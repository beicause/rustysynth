use crate::channel::Channel;
use crate::instrument_region::InstrumentRegion;
use crate::synthesizer_settings::SynthesizerSettings;
use crate::voice::Voice;

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct VoiceCollection {
    voices: Vec<Voice>,
    pub(crate) active_voice_count: usize,
}

impl VoiceCollection {
    pub(crate) fn new(settings: &SynthesizerSettings) -> Self {
        let mut voices: Vec<Voice> = Vec::with_capacity(settings.maximum_polyphony);
        for _i in 0..settings.maximum_polyphony {
            voices.push(Voice::new(settings));
        }

        Self {
            voices,
            active_voice_count: 0,
        }
    }

    pub(crate) fn request_new(
        &mut self,
        region: &InstrumentRegion,
        channel: i32,
    ) -> Option<&mut Voice> {
        // If an exclusive class is assigned to the region, find a voice with the same class.
        // If found, reuse it to avoid playing multiple voices with the same class at a time.
        let exclusive_class = region.get_exclusive_class();
        if exclusive_class != 0 {
            for i in 0..self.active_voice_count {
                let voice = &self.voices[i];
                if voice.exclusive_class() == exclusive_class && voice.channel() == channel {
                    return Some(&mut self.voices[i]);
                }
            }
        }

        // If the number of active voices is less than the limit, use a free one.
        if (self.active_voice_count) < self.voices.len() {
            let i = self.active_voice_count;
            self.active_voice_count += 1;
            return Some(&mut self.voices[i]);
        }

        // Too many active voices...
        // Find one which has the lowest priority.
        let mut candidate: usize = 0;
        let mut lowest_priority = f32::MAX;
        for i in 0..self.active_voice_count {
            let voice = &self.voices[i];
            let priority = voice.priority();
            if priority < lowest_priority {
                lowest_priority = priority;
                candidate = i;
            } else if priority == lowest_priority {
                // Same priority...
                // The older one should be more suitable for reuse.
                if voice.voice_length() > self.voices[candidate].voice_length() {
                    candidate = i;
                }
            }
        }
        Some(&mut self.voices[candidate])
    }

    pub(crate) fn process(&mut self, data: &[i16], channels: &[Channel]) {
        let alive = self.prepare_voices(data, channels);

        // A low-pass filter is a chain of dependent multiply-adds, so a single
        // one cannot fill the multiply-add unit. The voices are filtered four at
        // a time, which gives the core four independent chains to interleave.
        let mut group = [0_usize; 4];
        let mut in_group: usize = 0;
        for i in 0..alive {
            if self.voices[i].filter_is_active() {
                group[in_group] = i;
                in_group += 1;
                if in_group == 4 {
                    self.process_filter_group(group);
                    in_group = 0;
                }
            }
        }

        for &i in &group[..in_group] {
            self.voices[i].process_filter();
        }

        for i in 0..alive {
            if !self.voices[i].filter_is_active() {
                self.voices[i].process_filter();
            }
        }

        for voice in self.voices[..alive].iter_mut() {
            voice.finish(channels);
        }
    }

    /// Renders a block for every active voice and drops the finished ones,
    /// returning the number of voices left. The finished voices are removed with
    /// the same swaps as a straightforward pass would use, so the remaining
    /// voices keep the order they would have had.
    fn prepare_voices(&mut self, data: &[i16], channels: &[Channel]) -> usize {
        let mut count = self.active_voice_count;
        let mut i: usize = 0;

        while i < count {
            if self.voices[i].prepare(data, channels) {
                i += 1;
            } else {
                count -= 1;
                self.voices.swap(i, count);
            }
        }

        self.active_voice_count = count;
        count
    }

    fn process_filter_group(&mut self, mut indices: [usize; 4]) {
        indices.sort_unstable();
        let [i0, i1, i2, i3] = indices;

        let (head, tail) = self.voices.split_at_mut(i3);
        let v3 = &mut tail[0];
        let (head, tail) = head.split_at_mut(i2);
        let v2 = &mut tail[0];
        let (head, tail) = head.split_at_mut(i1);
        let v1 = &mut tail[0];
        let v0 = &mut head[i0];

        Voice::process_filters([v0, v1, v2, v3]);
    }

    pub(crate) fn get_active_voices(&mut self) -> &mut [Voice] {
        &mut self.voices[0..self.active_voice_count]
    }

    pub(crate) fn clear(&mut self) {
        self.active_voice_count = 0;
    }
}
