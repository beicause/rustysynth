use std::io::Read;

use crate::MidiFileError;
use crate::MidiFileLoopType;
use crate::binary_reader::BinaryReader;
use crate::four_cc::FourCC;
use crate::read_counter::ReadCounter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum Message {
    Normal { status: u8, data1: u8, data2: u8 },
    TempoChange { bytes: [u8; 3] },
    LoopStart,
    LoopEnd,
    EndOfTrack,
}

impl Message {
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
                    return Message::LoopStart;
                }

                MidiFileLoopType::IncredibleMachine => {
                    if data1 == 110 {
                        return Message::LoopStart;
                    }
                    if data1 == 111 {
                        return Message::LoopEnd;
                    }
                }

                MidiFileLoopType::FinalFantasy => {
                    if data1 == 116 {
                        return Message::LoopStart;
                    }
                    if data1 == 117 {
                        return Message::LoopEnd;
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
    pub(crate) messages: Vec<Message>,
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

        let mut message_lists: Vec<Vec<Message>> = Vec::with_capacity(track_count as usize);
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
                            message_list.insert(i, Message::LoopStart);
                            break;
                        }
                    }
                } else {
                    tick_list.push(loop_point);
                    message_list.push(Message::LoopStart);
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
    ) -> Result<(Vec<Message>, Vec<i32>), MidiFileError> {
        let chunk_type = BinaryReader::read_four_cc(reader)?;
        if chunk_type != b"MTrk" {
            return Err(MidiFileError::InvalidChunkType {
                expected: FourCC::from_bytes(*b"MTrk"),
                actual: chunk_type,
            });
        }

        let size = BinaryReader::read_i32_big_endian(reader)? as usize;
        let reader = &mut ReadCounter::new(reader);

        let mut messages: Vec<Message> = Vec::new();
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
                    messages.push(Message::common1(last_status, first));
                    ticks.push(tick);
                } else {
                    let data2 = BinaryReader::read_u8(reader)?;
                    messages.push(Message::common2(last_status, first, data2, loop_type));
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
                        messages.push(Message::EndOfTrack);
                        ticks.push(tick);

                        // Some MIDI files may have events inserted after the EOT.
                        // Such events should be ignored.
                        if reader.bytes_read() < size {
                            BinaryReader::discard_data(reader, size - reader.bytes_read())?;
                        }

                        return Ok((messages, ticks));
                    }
                    0x51 => {
                        messages.push(Message::tempo_change(MidiFile::read_tempo(reader)?));
                        ticks.push(tick);
                    }
                    _ => MidiFile::discard_data(reader)?,
                },
                _ => {
                    let command = first & 0xF0;
                    if command == 0xC0 || command == 0xD0 {
                        let data1 = BinaryReader::read_u8(reader)?;
                        messages.push(Message::common1(first, data1));
                        ticks.push(tick);
                    } else {
                        let data1 = BinaryReader::read_u8(reader)?;
                        let data2 = BinaryReader::read_u8(reader)?;
                        messages.push(Message::common2(first, data1, data2, loop_type));
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
        message_lists: &[Vec<Message>],
        tick_lists: &[Vec<i32>],
        resolution: i32,
    ) -> (Vec<Message>, Vec<f64>) {
        // Every merged message corresponds to one entry in one of the tick lists.
        let total_messages: usize = tick_lists.iter().map(Vec::len).sum();
        let mut merged_messages: Vec<Message> = Vec::with_capacity(total_messages);
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
            if let Message::TempoChange { bytes } = message {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_size() {
        // Avoid increasing the size of the Message type
        assert_eq!(size_of::<Message>(), 4);
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
            Message::Normal {
                status: 0x90,
                data1: 0x3C,
                data2: 0x64,
            }
        );
        assert_eq!(midi.messages[1], Message::EndOfTrack);
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
                Message::Normal {
                    status: 0x90,
                    data1: 0x3C,
                    data2: 0x64,
                },
                Message::Normal {
                    status: 0x90,
                    data1: 0x3D,
                    data2: 0x60,
                },
                Message::Normal {
                    status: 0x90,
                    data1: 0x3E,
                    data2: 0x50,
                },
                Message::EndOfTrack,
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
                Message::Normal {
                    status: 0x90,
                    data1: 0x3C,
                    data2: 0x64,
                },
                Message::Normal {
                    status: 0x00,
                    data1: 0x3D,
                    data2: 0x64,
                },
                Message::EndOfTrack,
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
    fn test_vlq_encoding() {
        assert_eq!(vlq(0), vec![0x00]);
        assert_eq!(vlq(0x40), vec![0x40]);
        assert_eq!(vlq(0x7F), vec![0x7F]);
        assert_eq!(vlq(0x80), vec![0x81, 0x00]);
        assert_eq!(vlq(480), vec![0x83, 0x60]);
        assert_eq!(vlq(0x1FFFFF), vec![0xFF, 0xFF, 0x7F]);
    }
}
