use crate::loop_mode::LoopMode;
use crate::synthesizer_settings::SynthesizerSettings;

// In this class, fixed-point numbers are used for speed-up.
// A fixed-point number is expressed by Int64, whose lower 24 bits represent the fraction part,
// and the rest represent the integer part.
// For clarity, fixed-point number variables have a suffix "_fp".

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct Oscillator {
    synthesizer_sample_rate: i32,

    loop_mode: LoopMode,
    sample_sample_rate: i32,
    start: i32,
    end: i32,
    start_loop: i32,
    end_loop: i32,
    root_key: i32,

    tune: f32,
    pitch_change_scale: f32,
    sample_rate_ratio: f32,

    looping: bool,

    position_fp: i64,

    // The pitch usually stays the same from block to block, but evaluating
    // 2^(pitch / 12) for every block costs more than the whole oscillator loop
    // for small blocks. The cached ratio is the exact value of the last pitch.
    cached_pitch: f32,
    cached_pitch_ratio_fp: i64,
    cached_pitch_is_valid: bool,
}

impl Oscillator {
    const FRAC_BITS: i32 = 24;
    const FRAC_UNIT: i64 = 1_i64 << Oscillator::FRAC_BITS;
    const FP_TO_SAMPLE: f32 = 1_f32 / (32768 * Oscillator::FRAC_UNIT) as f32;

    pub(crate) fn new(settings: &SynthesizerSettings) -> Self {
        Self {
            synthesizer_sample_rate: settings.sample_rate,
            loop_mode: LoopMode::NoLoop,
            sample_sample_rate: 0,
            start: 0,
            end: 0,
            start_loop: 0,
            end_loop: 0,
            root_key: 0,
            tune: 0_f32,
            pitch_change_scale: 0_f32,
            sample_rate_ratio: 0_f32,
            looping: false,
            position_fp: 0,
            cached_pitch: 0_f32,
            cached_pitch_ratio_fp: 0,
            cached_pitch_is_valid: false,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "oscillator needs the full parameter set"
    )]
    pub(crate) fn start(
        &mut self,
        loop_mode: LoopMode,
        sample_rate: i32,
        start: i32,
        end: i32,
        start_loop: i32,
        end_loop: i32,
        root_key: i32,
        coarse_tune: i32,
        fine_tune: i32,
        scale_tuning: i32,
    ) {
        self.loop_mode = loop_mode;
        self.sample_sample_rate = sample_rate;
        self.start = start;
        self.end = end;
        self.start_loop = start_loop;
        self.end_loop = end_loop;
        self.root_key = root_key;

        self.tune = coarse_tune as f32 + 0.01_f32 * fine_tune as f32;
        self.pitch_change_scale = 0.01_f32 * scale_tuning as f32;
        self.sample_rate_ratio = sample_rate as f32 / self.synthesizer_sample_rate as f32;
        // Looping is disabled when the loop range is invalid (start >= end),
        // so the region plays once instead of reading past the loop (#55).
        self.looping = self.loop_mode != LoopMode::NoLoop && self.start_loop < self.end_loop;
        self.position_fp = (start as i64) << Oscillator::FRAC_BITS;
        self.cached_pitch_is_valid = false;
    }

    pub(crate) fn release(&mut self) {
        if self.loop_mode == LoopMode::LoopUntilNoteOff {
            self.looping = false;
        }
    }

    pub(crate) fn process(&mut self, data: &[i16], block: &mut [f32], pitch: f32) -> bool {
        if !self.cached_pitch_is_valid || pitch != self.cached_pitch {
            let pitch_change = self.pitch_change_scale * (pitch - self.root_key as f32) + self.tune;
            let pitch_ratio = self.sample_rate_ratio * 2_f32.powf(pitch_change / 12_f32);

            // A pitch ratio that is zero, negative, not finite or absurdly large comes
            // from a corrupt SoundFont. Such a ratio would stall, rewind or overflow the
            // read position, so the voice is dropped instead of reading out of bounds.
            // https://github.com/sinshu/rustysynth/issues/57
            let ratio_limit = (i64::MAX / 2) as f64 / Oscillator::FRAC_UNIT as f64;
            let pitch_ratio_f64 = pitch_ratio as f64;
            if pitch_ratio.is_nan() || pitch_ratio_f64 <= 0_f64 || pitch_ratio_f64 >= ratio_limit {
                return false;
            }

            self.cached_pitch = pitch;
            self.cached_pitch_ratio_fp = (Oscillator::FRAC_UNIT as f64 * pitch_ratio as f64) as i64;
            self.cached_pitch_is_valid = true;
        }

        self.fill_block(data, block, self.cached_pitch_ratio_fp)
    }

    fn fill_block(&mut self, data: &[i16], block: &mut [f32], pitch_ratio_fp: i64) -> bool {
        if self.looping {
            self.fill_block_continuous(data, block, pitch_ratio_fp)
        } else {
            self.fill_block_no_loop(data, block, pitch_ratio_fp)
        }
    }

    fn fill_block_no_loop(&mut self, data: &[i16], block: &mut [f32], pitch_ratio_fp: i64) -> bool {
        let end = self.end as usize;

        // Restrict the slice to the sample data the loop can touch, so the bounds
        // checks on `data` fold away. If the data is shorter than `end` the
        // original indexing behaviour (and its panic) is kept.
        let data = if data.len() > end {
            &data[..end + 1]
        } else {
            data
        };

        let mut position_fp = self.position_fp;

        for t in 0..block.len() {
            let index = (position_fp >> Oscillator::FRAC_BITS) as usize;
            if index >= end {
                self.position_fp = position_fp;
                if t > 0 {
                    let len = block.len();
                    block[t..len].fill(0_f32);
                    return true;
                } else {
                    return false;
                }
            }

            let x1 = data[index] as i64;
            let x2 = data[index + 1] as i64;
            let a_fp = position_fp & (Oscillator::FRAC_UNIT - 1);
            block[t] = Oscillator::FP_TO_SAMPLE
                * ((x1 << Oscillator::FRAC_BITS) + a_fp * (x2 - x1)) as f32;

            position_fp += pitch_ratio_fp;
        }

        self.position_fp = position_fp;

        true
    }

    fn fill_block_continuous(
        &mut self,
        data: &[i16],
        block: &mut [f32],
        pitch_ratio_fp: i64,
    ) -> bool {
        let end_loop = self.end_loop as usize;
        let end_loop_fp = (self.end_loop as i64) << Oscillator::FRAC_BITS;
        let loop_length = (self.end_loop - self.start_loop) as i64;
        let loop_length_fp = loop_length << Oscillator::FRAC_BITS;
        let loop_length_usize = loop_length as usize;

        // Slicing to the loop end lets the optimizer prove that both indices are
        // in bounds, which removes the per-sample bounds checks.
        let data = match data.get(..end_loop) {
            Some(sliced) => sliced,
            None => data,
        };

        let mut position_fp = self.position_fp;

        // Looping is only enabled for a non-empty loop range, but a corrupt
        // region must not be able to divide by zero below.
        if loop_length_fp <= 0 {
            return false;
        }

        // A block that stays two samples short of the loop end needs neither the
        // wrap test nor the test for the second index falling off the loop, which
        // is the case for almost every block. Both are left out of the loop
        // below, which removes a compare, a conditional subtract and a compare
        // with a conditional add from every sample.
        let last_position_fp = (block.len() as i64 - 1)
            .saturating_mul(pitch_ratio_fp)
            .saturating_add(position_fp);
        if last_position_fp < end_loop_fp - Oscillator::FRAC_UNIT {
            for sample in block.iter_mut() {
                let index1 = (position_fp >> Oscillator::FRAC_BITS) as usize;

                let x1 = data[index1] as i64;
                let x2 = data[index1 + 1] as i64;
                let a_fp = position_fp & (Oscillator::FRAC_UNIT - 1);
                *sample = Oscillator::FP_TO_SAMPLE
                    * ((x1 << Oscillator::FRAC_BITS) + a_fp * (x2 - x1)) as f32;

                position_fp += pitch_ratio_fp;
            }

            self.position_fp = position_fp;

            return true;
        }

        for sample in block.iter_mut() {
            if position_fp >= end_loop_fp {
                // A loop shorter than the distance the read position advances per
                // sample needs more than one wrap. Wrapping only once used to leave
                // the position past the loop and eventually read out of bounds.
                // The remainder keeps the arithmetic identical for loops that are
                // long enough, so healthy SoundFonts render bit for bit the same.
                // https://github.com/sinshu/rustysynth/issues/57
                let overshoot = position_fp - end_loop_fp;
                position_fp = end_loop_fp - loop_length_fp + overshoot % loop_length_fp;
            }

            let index1 = (position_fp >> Oscillator::FRAC_BITS) as usize;
            let mut index2 = index1 + 1;
            if index2 >= end_loop {
                index2 -= loop_length_usize;
            }

            let x1 = data[index1] as i64;
            let x2 = data[index2] as i64;
            let a_fp = position_fp & (Oscillator::FRAC_UNIT - 1);
            *sample = Oscillator::FP_TO_SAMPLE
                * ((x1 << Oscillator::FRAC_BITS) + a_fp * (x2 - x1)) as f32;

            position_fp += pitch_ratio_fp;
        }

        self.position_fp = position_fp;

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oscillator(start_loop: i32, end_loop: i32, sample_rate: i32) -> Oscillator {
        let settings = SynthesizerSettings::new(44100);
        let mut oscillator = Oscillator::new(&settings);
        oscillator.start(
            LoopMode::Continuous,
            sample_rate,
            0,
            4000,
            start_loop,
            end_loop,
            60,
            0,
            0,
            100,
        );
        oscillator
    }

    // A loop of a single frame advances by more than its own length per sample
    // at a high pitch. Wrapping once per sample left the read position drifting
    // further past the loop on every sample until it indexed out of bounds
    // (#57, reported with a two frame Harpsichord loop in Arachno).
    #[test]
    fn test_a_loop_shorter_than_the_step_does_not_read_out_of_bounds() {
        let data = vec![1000_i16; 4096];
        let mut oscillator = oscillator(1000, 1001, 44100);
        let mut block = vec![0_f32; 256];

        // An octave and a half above the root key, so the step is about 4 frames.
        assert!(oscillator.process(&data, &mut block, 78_f32));
        assert!(block.iter().all(|sample| sample.is_finite()));

        // Keep rendering: the error used to show up after the loop had been
        // overshot a few times rather than on the very first block.
        for _ in 0..64 {
            assert!(oscillator.process(&data, &mut block, 78_f32));
        }
    }

    // A raised pitch advances the read position by more than one frame per
    // sample, so a block can cross the loop end in the middle of it. The fast
    // path is only taken while the whole block stays inside the loop, so the
    // frames it renders must match the ones the per-sample path renders for the
    // same positions.
    #[test]
    fn test_a_raised_pitch_crossing_the_loop_end_matches_the_per_sample_path() {
        let data: Vec<i16> = (0..4096).map(|i| ((i * 13) % 2000) as i16).collect();

        // Two semitones above the root key at this scale tuning is a step of
        // two frames, so a 64 sample block walks past the 100 frame loop.
        let mut one_block = oscillator(1000, 1100, 44100);
        let mut block = vec![0_f32; 64];
        assert!(one_block.process(&data, &mut block, 72_f32));

        let mut per_sample = oscillator(1000, 1100, 44100);
        let mut single = vec![0_f32; 1];
        let mut expected = Vec::with_capacity(block.len());

        for _ in 0..block.len() {
            assert!(per_sample.process(&data, &mut single, 72_f32));
            expected.push(single[0]);
        }

        assert_eq!(block, expected);
    }

    // A corrupt sample rate or pitch must not leave the read position stall,
    // rewind or overflow, so the voice is dropped instead (#57).
    #[test]
    fn test_an_invalid_pitch_ratio_drops_the_voice() {
        let data = vec![1000_i16; 4096];
        let mut block = vec![0_f32; 64];

        let mut stalled = oscillator(1000, 2000, 0);
        assert!(!stalled.process(&data, &mut block, 60_f32));

        let mut reversed = oscillator(1000, 2000, 44100);
        assert!(!reversed.process(&data, &mut block, f32::NAN));
    }

    // An empty loop range is played once instead of being looped (#55).
    #[test]
    fn test_an_empty_loop_range_is_played_once() {
        let data = vec![1000_i16; 4096];
        let mut oscillator = oscillator(1000, 1000, 44100);
        let mut block = vec![0_f32; 64];

        assert!(!oscillator.looping);
        assert!(oscillator.process(&data, &mut block, 60_f32));
    }
}
