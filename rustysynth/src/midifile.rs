use std::cmp::Ordering;
use std::io::Read;

use crate::MidiFileError;
use crate::MidiFileLoopType;
use crate::binary_reader::BinaryReader;
use crate::four_cc::FourCC;
use crate::read_counter::ReadCounter;

/// A single event of a merged MIDI file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MidiMessage {
    /// A channel message; `status` includes the channel in its low nibble.
    Normal { status: u8, data1: u8, data2: u8 },
    /// A tempo change in microseconds per quarter note (FF 51 03).
    ///
    /// Only seen while parsing; tempo changes are folded into the event times
    /// and never appear in the merged list.
    TempoChange { bytes: [u8; 3] },
    /// Loop start marker (loop extensions only).
    LoopStart,
    /// Loop end marker (loop extensions only).
    LoopEnd,
    /// End of track.
    EndOfTrack,
}

impl MidiMessage {
    pub(crate) fn common1(status: u8, data1: u8) -> Self {
        Self::Normal {
            status,
            data1,
            data2: 0,
        }
    }

    pub(crate) fn common2(status: u8, data1: u8, data2: u8, loop_type: MidiFileLoopType) -> Self {
        let command = status & 0xF0;

        if command == 0xB0 {
            match loop_type {
                MidiFileLoopType::RpgMaker if data1 == 111 => {
                    return MidiMessage::LoopStart;
                }

                MidiFileLoopType::IncredibleMachine => {
                    if data1 == 110 {
                        return MidiMessage::LoopStart;
                    }
                    if data1 == 111 {
                        return MidiMessage::LoopEnd;
                    }
                }

                MidiFileLoopType::FinalFantasy => {
                    if data1 == 116 {
                        return MidiMessage::LoopStart;
                    }
                    if data1 == 117 {
                        return MidiMessage::LoopEnd;
                    }
                }

                _ => (),
            }
        }

        Self::Normal {
            status,
            data1,
            data2,
        }
    }

    pub(crate) fn tempo_change(tempo: i32) -> Self {
        // Truncate to u24
        let bytes = tempo.to_be_bytes()[1..].try_into().unwrap();
        Self::TempoChange { bytes }
    }
}

/// Represents a standard MIDI file.
#[derive(Debug)]
#[non_exhaustive]
pub struct MidiFile {
    pub(crate) messages: Vec<MidiMessage>,
    pub(crate) times: Vec<f64>,
}

impl MidiFile {
    /// Loads a MIDI file from the stream.
    ///
    /// # Arguments
    ///
    /// * `reader` - The data stream used to load the MIDI file.
    pub fn new<R: Read>(reader: &mut R) -> Result<Self, MidiFileError> {
        MidiFile::new_with_loop_type(reader, MidiFileLoopType::LoopPoint(0))
    }

    /// Loads a MIDI file from the stream with a specified loop type.
    ///
    /// # Arguments
    ///
    /// * `reader` - The data stream used to load the MIDI file.
    /// * `loop_type` - The type of the loop extension to be used.
    ///
    /// # Remarks
    ///
    /// `MidiFileLoopType` has the following variants:
    /// * `LoopPoint(usize)` - Specifies the loop start point by a tick value.
    /// * `RpgMaker` - The RPG Maker style loop.
    ///   CC #111 will be the loop start point.
    /// * `IncredibleMachine` - The Incredible Machine style loop.
    ///   CC #110 and #111 will be the start and end points of the loop.
    /// * `FinalFantasy` - The Final Fantasy style loop.
    ///   CC #116 and #117 will be the start and end points of the loop.
    pub fn new_with_loop_type<R: Read>(
        reader: &mut R,
        loop_type: MidiFileLoopType,
    ) -> Result<Self, MidiFileError> {
        let chunk_type = BinaryReader::read_four_cc(reader)?;
        if chunk_type != b"MThd" {
            return Err(MidiFileError::InvalidChunkType {
                expected: FourCC::from_bytes(*b"MThd"),
                actual: chunk_type,
            });
        }

        let size = BinaryReader::read_i32_big_endian(reader)?;
        if size != 6 {
            return Err(MidiFileError::InvalidChunkData(FourCC::from_bytes(
                *b"MThd",
            )));
        }

        let format = BinaryReader::read_i16_big_endian(reader)?;
        if !(format == 0 || format == 1) {
            return Err(MidiFileError::UnsupportedFormat(format));
        }

        // The SMF specification requires at least one track chunk.
        let track_count = BinaryReader::read_i16_big_endian(reader)?;
        if track_count <= 0 {
            return Err(MidiFileError::InvalidChunkData(FourCC::from_bytes(
                *b"MThd",
            )));
        }

        // The time division must be a positive number of ticks per quarter
        // note. SMPTE timecode divisions are encoded with bit 15 set (i.e. a
        // negative value here) and are not supported.
        let resolution = BinaryReader::read_i16_big_endian(reader)?;
        if resolution <= 0 {
            return Err(MidiFileError::InvalidTimeDivision(resolution));
        }
        let track_count = track_count as i32;
        let resolution = resolution as i32;

        let mut message_lists: Vec<Vec<MidiMessage>> = Vec::with_capacity(track_count as usize);
        let mut tick_lists: Vec<Vec<i32>> = Vec::with_capacity(track_count as usize);

        for _i in 0..track_count {
            let (message_list, tick_list) = MidiFile::read_track(reader, loop_type)?;
            message_lists.push(message_list);
            tick_lists.push(tick_list);
        }

        match loop_type {
            MidiFileLoopType::LoopPoint(loop_point) if loop_point != 0 => {
                let loop_point = loop_point as i32;
                let tick_list = &mut tick_lists[0];
                let message_list = &mut message_lists[0];

                if loop_point <= *tick_list.last().unwrap() {
                    for i in 0..tick_list.len() {
                        if tick_list[i] >= loop_point {
                            tick_list.insert(i, loop_point);
                            message_list.insert(i, MidiMessage::LoopStart);
                            break;
                        }
                    }
                } else {
                    tick_list.push(loop_point);
                    message_list.push(MidiMessage::LoopStart);
                }
            }
            _ => (),
        }

        let (messages, times) = MidiFile::merge_tracks(&message_lists, &tick_lists, resolution);

        Ok(Self { messages, times })
    }

    fn discard_data<R: Read>(reader: &mut R) -> Result<(), MidiFileError> {
        let size = BinaryReader::read_i32_variable_length(reader)? as usize;
        BinaryReader::discard_data(reader, size)?;
        Ok(())
    }

    fn read_tempo<R: Read>(reader: &mut R) -> Result<i32, MidiFileError> {
        let size = BinaryReader::read_i32_variable_length(reader)?;
        if size != 3 {
            return Err(MidiFileError::InvalidTempoValue);
        }

        let b1 = BinaryReader::read_u8(reader)? as i32;
        let b2 = BinaryReader::read_u8(reader)? as i32;
        let b3 = BinaryReader::read_u8(reader)? as i32;

        Ok((b1 << 16) | (b2 << 8) | b3)
    }

    fn read_track<R: Read>(
        reader: &mut R,
        loop_type: MidiFileLoopType,
    ) -> Result<(Vec<MidiMessage>, Vec<i32>), MidiFileError> {
        let chunk_type = BinaryReader::read_four_cc(reader)?;
        if chunk_type != b"MTrk" {
            return Err(MidiFileError::InvalidChunkType {
                expected: FourCC::from_bytes(*b"MTrk"),
                actual: chunk_type,
            });
        }

        let size = BinaryReader::read_i32_big_endian(reader)? as usize;
        let reader = &mut ReadCounter::new(reader);

        let mut messages: Vec<MidiMessage> = Vec::new();
        let mut ticks: Vec<i32> = Vec::new();

        let mut tick: i32 = 0;
        let mut last_status: u8 = 0;

        loop {
            let delta = BinaryReader::read_i32_variable_length(reader)?;
            let first = BinaryReader::read_u8(reader)?;

            tick += delta;

            if (first & 128) == 0 {
                let command = last_status & 0xF0;
                if command == 0xC0 || command == 0xD0 {
                    messages.push(MidiMessage::common1(last_status, first));
                    ticks.push(tick);
                } else {
                    let data2 = BinaryReader::read_u8(reader)?;
                    messages.push(MidiMessage::common2(last_status, first, data2, loop_type));
                    ticks.push(tick);
                }

                continue;
            }

            match first {
                0xF0 => MidiFile::discard_data(reader)?,
                0xF7 => MidiFile::discard_data(reader)?,
                0xFF => match BinaryReader::read_u8(reader)? {
                    0x2F => {
                        BinaryReader::read_u8(reader)?;
                        messages.push(MidiMessage::EndOfTrack);
                        ticks.push(tick);

                        // Some MIDI files may have events inserted after the EOT.
                        // Such events should be ignored.
                        if reader.bytes_read() < size {
                            BinaryReader::discard_data(reader, size - reader.bytes_read())?;
                        }

                        return Ok((messages, ticks));
                    }
                    0x51 => {
                        messages.push(MidiMessage::tempo_change(MidiFile::read_tempo(reader)?));
                        ticks.push(tick);
                    }
                    _ => MidiFile::discard_data(reader)?,
                },
                _ => {
                    let command = first & 0xF0;
                    if command == 0xC0 || command == 0xD0 {
                        let data1 = BinaryReader::read_u8(reader)?;
                        messages.push(MidiMessage::common1(first, data1));
                        ticks.push(tick);
                    } else {
                        let data1 = BinaryReader::read_u8(reader)?;
                        let data2 = BinaryReader::read_u8(reader)?;
                        messages.push(MidiMessage::common2(first, data1, data2, loop_type));
                        ticks.push(tick);
                    }
                }
            }

            // Per the SMF specification, only channel messages update the
            // running status byte; SysEx and Meta events interrupt (clear) it.
            last_status = if first >= 0xF0 { 0 } else { first };
        }
    }

    fn merge_tracks(
        message_lists: &[Vec<MidiMessage>],
        tick_lists: &[Vec<i32>],
        resolution: i32,
    ) -> (Vec<MidiMessage>, Vec<f64>) {
        // Every merged message corresponds to one entry in one of the tick lists.
        let total_messages: usize = tick_lists.iter().map(Vec::len).sum();
        let mut merged_messages: Vec<MidiMessage> = Vec::with_capacity(total_messages);
        let mut merged_times: Vec<f64> = Vec::with_capacity(total_messages);

        let mut indices: Vec<usize> = vec![0; message_lists.len()];

        let mut current_tick: i32 = 0;
        let mut current_time: f64 = 0.0;

        let mut tempo: f64 = 120.0;

        loop {
            let mut min_tick = i32::MAX;
            let mut min_index: i32 = -1;

            for ch in 0..tick_lists.len() {
                if indices[ch] < tick_lists[ch].len() {
                    let tick = tick_lists[ch][indices[ch]];
                    if tick < min_tick {
                        min_tick = tick;
                        min_index = ch as i32;
                    }
                }
            }

            if min_index == -1 {
                break;
            }

            let next_tick = tick_lists[min_index as usize][indices[min_index as usize]];
            let delta_tick = next_tick - current_tick;
            let delta_time = 60.0 / (resolution as f64 * tempo) * delta_tick as f64;

            current_tick += delta_tick;
            current_time += delta_time;

            let message = message_lists[min_index as usize][indices[min_index as usize]];
            if let MidiMessage::TempoChange { bytes } = message {
                let tempo_i32 = i32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
                // A tempo of 0 microseconds per quarter note is invalid data;
                // keep the previous tempo instead of dividing by zero.
                if tempo_i32 > 0 {
                    tempo = 60000000.0 / tempo_i32 as f64;
                }
            } else {
                merged_messages.push(message);
                merged_times.push(current_time);
            }

            indices[min_index as usize] += 1;
        }

        (merged_messages, merged_times)
    }

    /// Get the length of the MIDI file in seconds.
    pub fn get_length(&self) -> f64 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// Get the merged message list of the MIDI file.
    ///
    /// Parallel to [`get_times`](Self::get_times): message *i* occurs at
    /// `get_times()[i]`. Tempo changes are already folded into the times.
    pub fn get_messages(&self) -> &[MidiMessage] {
        &self.messages
    }

    /// Get the time of every message in absolute seconds.
    ///
    /// Parallel to [`get_messages`](Self::get_messages).
    pub fn get_times(&self) -> &[f64] {
        &self.times
    }

    /// Creates a MIDI file from an event list without parsing.
    ///
    /// Times must be in non-decreasing order (same-time events allowed);
    /// returns [`MidiFileError::InvalidEventList`] otherwise. Accepts any
    /// iterator of `(time, message)` pairs.
    pub fn new_with_events<I>(events: I) -> Result<Self, MidiFileError>
    where
        I: IntoIterator<Item = (f64, MidiMessage)>,
    {
        let mut file = Self::default();
        file.extend_events(events)?;
        Ok(file)
    }

    /// Appends events to the end of the MIDI file.
    ///
    /// Appended times must be non-decreasing and not earlier than the last
    /// existing time; otherwise returns [`MidiFileError::InvalidEventList`]
    /// and leaves the file unchanged. To replace all events, call
    /// [`clear`](Self::clear) first.
    pub fn extend_events<I>(&mut self, events: I) -> Result<(), MidiFileError>
    where
        I: IntoIterator<Item = (f64, MidiMessage)>,
    {
        let base = self.messages.len();
        let mut last_time = self.times.last().copied().unwrap_or(f64::NEG_INFINITY);

        let iter = events.into_iter();
        let (min, max) = iter.size_hint();
        // `Vec::reserve` is a no-op when the current capacity already
        // suffices, so appending a similar amount of events reuses the
        // existing allocation.
        let capacity = max.unwrap_or(min);
        self.messages.reserve(capacity);
        self.times.reserve(capacity);

        for (time, message) in iter {
            // `partial_cmp` yields `None` for NaN, so NaN times are also
            // rejected here.
            if !matches!(
                time.partial_cmp(&last_time),
                Some(Ordering::Greater | Ordering::Equal)
            ) {
                self.messages.truncate(base);
                self.times.truncate(base);
                return Err(MidiFileError::InvalidEventList);
            }
            last_time = time;
            self.messages.push(message);
            self.times.push(time);
        }
        Ok(())
    }

    /// Removes all events from the MIDI file.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.times.clear();
    }
}

impl Default for MidiFile {
    /// Creates an empty MIDI file.
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            times: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_size() {
        // Avoid increasing the size of the MidiMessage type
        assert_eq!(size_of::<MidiMessage>(), 4);
    }

    /// Encodes a value as a MIDI variable-length quantity.
    fn vlq(mut value: u32) -> Vec<u8> {
        let mut groups = Vec::new();
        loop {
            groups.push((value & 0x7F) as u8);
            value >>= 7;
            if value == 0 {
                break;
            }
        }
        groups.reverse();
        let last = groups.len() - 1;
        for (i, byte) in groups.iter_mut().enumerate() {
            if i != last {
                *byte |= 0x80;
            }
        }
        groups
    }

    /// Builds an `MThd` header chunk.
    fn mthd(format: u16, track_count: u16, resolution: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&format.to_be_bytes());
        bytes.extend_from_slice(&track_count.to_be_bytes());
        bytes.extend_from_slice(&resolution.to_be_bytes());
        bytes
    }

    /// Appends an `MTrk` chunk with the given event data.
    fn mtrk(bytes: &mut Vec<u8>, data: &[u8]) {
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(data);
    }

    // Events: delta 480: note on (60, 100); delta 480: end of track.
    // PPQ 480 at the default 120 BPM: 480 ticks = 0.5 s.
    fn note_at_480() -> Vec<u8> {
        let mut data = mthd(0, 1, 480);
        let mut track = vlq(480);
        track.extend_from_slice(&[0x90, 0x3C, 0x64]);
        track.extend_from_slice(&vlq(480));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        mtrk(&mut data, &track);
        data
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, but was {actual}"
        );
    }

    #[test]
    fn test_default_tempo_timing() {
        let data = note_at_480();
        let midi = MidiFile::new(&mut data.as_slice()).unwrap();

        assert_eq!(midi.messages.len(), 2);
        assert_eq!(
            midi.messages[0],
            MidiMessage::Normal {
                status: 0x90,
                data1: 0x3C,
                data2: 0x64,
            }
        );
        assert_eq!(midi.messages[1], MidiMessage::EndOfTrack);
        assert_eq!(midi.times.len(), 2);
        assert_close(midi.times[0], 0.5);
        assert_close(midi.times[1], 1.0);
        assert_close(midi.get_length(), 1.0);
    }

    #[test]
    fn test_tempo_change_timing() {
        // Tempo change to 250000 us/quarter = 240 BPM at tick 0, then a note
        // at tick 480 (0.25 s at 240 BPM) and EOT at tick 960 (0.5 s).
        let mut data = mthd(0, 1, 480);
        let mut track = vlq(0);
        track.extend_from_slice(&[0xFF, 0x51, 0x03, 0x03, 0xD0, 0x90]);
        track.extend_from_slice(&vlq(480));
        track.extend_from_slice(&[0x90, 0x3C, 0x64]);
        track.extend_from_slice(&vlq(480));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        mtrk(&mut data, &track);

        let midi = MidiFile::new(&mut data.as_slice()).unwrap();

        // The tempo change is consumed during the merge and not emitted.
        assert_eq!(midi.messages.len(), 2);
        assert_close(midi.times[0], 0.25);
        assert_close(midi.times[1], 0.5);
        assert_close(midi.get_length(), 0.5);
    }

    #[test]
    fn test_invalid_tempo_value_is_ignored() {
        // A tempo of 0 us/quarter is invalid; the previous tempo (the 120 BPM
        // default) must be kept, so timing stays intact instead of collapsing.
        let mut data = mthd(0, 1, 480);
        let mut track = vlq(0);
        track.extend_from_slice(&[0xFF, 0x51, 0x03, 0x00, 0x00, 0x00]);
        track.extend_from_slice(&vlq(480));
        track.extend_from_slice(&[0x90, 0x3C, 0x64]);
        track.extend_from_slice(&vlq(480));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        mtrk(&mut data, &track);

        let midi = MidiFile::new(&mut data.as_slice()).unwrap();
        assert_eq!(midi.messages.len(), 2);
        assert_close(midi.times[0], 0.5);
        assert_close(midi.times[1], 1.0);
    }

    #[test]
    fn test_running_status_within_channel_messages() {
        // Three note-on events in a row using running status.
        let mut data = mthd(0, 1, 480);
        let mut track = vlq(0);
        track.extend_from_slice(&[0x90, 0x3C, 0x64]); // full status
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0x3D, 0x60]); // running status
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0x3E, 0x50]); // running status
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        mtrk(&mut data, &track);

        let midi = MidiFile::new(&mut data.as_slice()).unwrap();
        assert_eq!(
            midi.messages,
            vec![
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 0x3C,
                    data2: 0x64,
                },
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 0x3D,
                    data2: 0x60,
                },
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 0x3E,
                    data2: 0x50,
                },
                MidiMessage::EndOfTrack,
            ]
        );
    }

    #[test]
    fn test_running_status_cleared_by_meta_event() {
        // Per the SMF specification, Meta events interrupt (clear) running
        // status, so the data byte after the tempo event must not reuse the
        // previous channel status. It is parsed with status 0 instead.
        let mut data = mthd(0, 1, 480);
        let mut track = vlq(0);
        track.extend_from_slice(&[0x90, 0x3C, 0x64]);
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 500000 us
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0x3D, 0x64]); // data byte after a Meta event
        track.extend_from_slice(&vlq(0));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        mtrk(&mut data, &track);

        let midi = MidiFile::new(&mut data.as_slice()).unwrap();
        assert_eq!(
            midi.messages,
            vec![
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 0x3C,
                    data2: 0x64,
                },
                MidiMessage::Normal {
                    status: 0x00,
                    data1: 0x3D,
                    data2: 0x64,
                },
                MidiMessage::EndOfTrack,
            ]
        );
    }

    #[test]
    fn test_smpte_time_division_is_rejected() {
        // Bit 15 set => SMPTE timecode division (0xE7 0x28 = 25 fps x 40).
        // Negative resolutions cannot produce meaningful tick-to-second
        // timing, so the file is rejected with a clear error.
        let data = mthd(0, 1, 0xE728);
        assert!(matches!(
            MidiFile::new(&mut data.as_slice()),
            Err(MidiFileError::InvalidTimeDivision(-6360))
        ));
    }

    #[test]
    fn test_zero_time_division_is_rejected() {
        let data = mthd(0, 1, 0x0000);
        assert!(matches!(
            MidiFile::new(&mut data.as_slice()),
            Err(MidiFileError::InvalidTimeDivision(0))
        ));
    }

    #[test]
    fn test_empty_track_list_is_rejected() {
        // The SMF specification requires at least one track chunk.
        let data = mthd(0, 0, 480);
        assert!(matches!(
            MidiFile::new(&mut data.as_slice()),
            Err(MidiFileError::InvalidChunkData(_))
        ));
    }

    #[test]
    fn test_message_accessors() {
        let data = note_at_480();
        let midi = MidiFile::new(&mut data.as_slice()).unwrap();

        assert_eq!(midi.get_messages().len(), 2);
        assert_eq!(midi.get_times().len(), 2);
        assert_close(midi.get_times()[0], 0.5);
        assert_close(midi.get_times()[1], 1.0);

        let events: Vec<(f64, MidiMessage)> = midi
            .get_times()
            .iter()
            .copied()
            .zip(midi.get_messages().iter().copied())
            .collect();
        assert_eq!(events.len(), 2);
        assert_close(events[0].0, 0.5);
        assert_eq!(
            events[0].1,
            MidiMessage::Normal {
                status: 0x90,
                data1: 0x3C,
                data2: 0x64,
            }
        );
        assert_close(events[1].0, 1.0);
        assert_eq!(events[1].1, MidiMessage::EndOfTrack);
    }

    #[test]
    fn test_construct_with_events() {
        let midi = MidiFile::new_with_events(vec![
            (
                0.25,
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 60,
                    data2: 100,
                },
            ),
            (0.5, MidiMessage::EndOfTrack),
        ])
        .unwrap();

        assert_eq!(midi.get_messages().len(), 2);
        assert_eq!(midi.get_times(), &[0.25, 0.5]);
        assert_close(midi.get_length(), 0.5);
        assert_close(midi.get_times()[0], 0.25);
        assert_eq!(
            midi.get_messages()[0],
            MidiMessage::Normal {
                status: 0x90,
                data1: 60,
                data2: 100,
            }
        );
    }

    #[test]
    fn test_construct_allows_same_time_events() {
        let midi = MidiFile::new_with_events(vec![
            (
                0.0,
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 60,
                    data2: 100,
                },
            ),
            (
                0.0,
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 61,
                    data2: 100,
                },
            ),
        ])
        .unwrap();
        assert_eq!(midi.get_messages().len(), 2);
    }

    #[test]
    fn test_construct_from_iterator_without_vec() {
        // Any iterator of (time, message) pairs is accepted, so the caller
        // never has to build an intermediate Vec.
        let midi = MidiFile::new_with_events((0..3).map(|i| {
            (
                i as f64 * 0.5,
                MidiMessage::Normal {
                    status: 0x90,
                    data1: i as u8,
                    data2: 100,
                },
            )
        }))
        .unwrap();
        assert_eq!(midi.get_messages().len(), 3);
        assert_eq!(midi.get_times(), &[0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_construct_rejects_unsorted_times() {
        let result = MidiFile::new_with_events(vec![
            (0.5, MidiMessage::EndOfTrack),
            (
                0.25,
                MidiMessage::Normal {
                    status: 0x90,
                    data1: 60,
                    data2: 100,
                },
            ),
        ]);
        assert!(matches!(result, Err(MidiFileError::InvalidEventList)));
    }

    #[test]
    fn test_construct_rejects_nan_time() {
        let result = MidiFile::new_with_events(vec![(f64::NAN, MidiMessage::EndOfTrack)]);
        assert!(matches!(result, Err(MidiFileError::InvalidEventList)));
    }

    #[test]
    fn test_extend_events_and_clear() {
        let mut midi = MidiFile::default();
        assert!(midi.get_messages().is_empty());
        assert_close(midi.get_length(), 0.0);

        midi.extend_events(vec![(0.5, MidiMessage::EndOfTrack)])
            .unwrap();
        assert_eq!(midi.get_times(), &[0.5]);
        assert_close(midi.get_length(), 0.5);

        // Appending keeps the existing events; same-time events are allowed.
        midi.extend_events(vec![(0.5, MidiMessage::EndOfTrack)])
            .unwrap();
        assert_eq!(midi.get_messages().len(), 2);
        assert_eq!(midi.get_times(), &[0.5, 0.5]);

        midi.clear();
        assert!(midi.get_messages().is_empty());
        assert_close(midi.get_length(), 0.0);
    }

    #[test]
    fn test_extend_events_reuses_allocation() {
        let mut midi = MidiFile::default();

        let events: Vec<(f64, MidiMessage)> = (0..100)
            .map(|i| {
                (
                    i as f64 * 0.01,
                    MidiMessage::Normal {
                        status: 0x90,
                        data1: i as u8,
                        data2: 0,
                    },
                )
            })
            .collect();
        midi.extend_events(events.iter().copied()).unwrap();
        let messages_capacity = midi.messages.capacity();
        let times_capacity = midi.times.capacity();
        assert!(messages_capacity >= 100);
        assert!(times_capacity >= 100);

        // clear() + extend() is the way to replace all events; it must reuse
        // the existing buffers instead of allocating new ones.
        midi.clear();
        midi.extend_events(vec![(0.0, MidiMessage::EndOfTrack)])
            .unwrap();
        assert_eq!(midi.messages.capacity(), messages_capacity);
        assert_eq!(midi.times.capacity(), times_capacity);
        assert_eq!(midi.get_messages().len(), 1);
        assert_eq!(midi.get_times(), &[0.0]);
    }

    #[test]
    fn test_extend_events_invalid_input_is_transactional() {
        let mut midi = MidiFile::new_with_events(vec![(0.0, MidiMessage::EndOfTrack)]).unwrap();

        // Out-of-order input (earlier than an existing event, or descending
        // within the batch) must leave the file unchanged.
        let result = midi.extend_events(vec![
            (0.5, MidiMessage::EndOfTrack),
            (0.25, MidiMessage::EndOfTrack),
        ]);
        assert!(matches!(result, Err(MidiFileError::InvalidEventList)));
        assert_eq!(midi.get_messages().len(), 1);
        assert_eq!(midi.get_times(), &[0.0]);

        // An event earlier than the last existing one (0.0) must also be
        // rejected, leaving the file unchanged.
        let result = midi.extend_events(vec![(-0.5, MidiMessage::EndOfTrack)]);
        assert!(matches!(result, Err(MidiFileError::InvalidEventList)));
        assert_eq!(midi.get_times(), &[0.0]);
    }

    #[test]
    fn test_vlq_encoding() {
        assert_eq!(vlq(0), vec![0x00]);
        assert_eq!(vlq(0x40), vec![0x40]);
        assert_eq!(vlq(0x7F), vec![0x7F]);
        assert_eq!(vlq(0x80), vec![0x81, 0x00]);
        assert_eq!(vlq(480), vec![0x83, 0x60]);
        assert_eq!(vlq(0x1FFFFF), vec![0xFF, 0xFF, 0x7F]);
    }
}
