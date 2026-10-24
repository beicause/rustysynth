use std::f32::consts;

use crate::synthesizer_settings::SynthesizerSettings;

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct BiQuadFilter {
    sample_rate: i32,

    active: bool,

    a0: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    a4: f32,

    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiQuadFilter {
    const RESONANCE_PEAK_OFFSET: f32 = 1_f32 - 1_f32 / core::f32::consts::SQRT_2;

    pub(crate) fn new(settings: &SynthesizerSettings) -> Self {
        Self {
            sample_rate: settings.sample_rate,
            active: false,
            a0: 0_f32,
            a1: 0_f32,
            a2: 0_f32,
            a3: 0_f32,
            a4: 0_f32,
            x1: 0_f32,
            x2: 0_f32,
            y1: 0_f32,
            y2: 0_f32,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn clear_buffer(&mut self) {
        self.x1 = 0_f32;
        self.x2 = 0_f32;
        self.y1 = 0_f32;
        self.y2 = 0_f32;
    }

    pub(crate) fn set_low_pass_filter(&mut self, cutoff_frequency: f32, resonance: f32) {
        if cutoff_frequency < 0.499_f32 * self.sample_rate as f32 {
            self.active = true;

            // This equation gives the Q value which makes the desired resonance peak.
            // The error of the resultant peak height is less than 3%.
            let q = resonance
                - BiQuadFilter::RESONANCE_PEAK_OFFSET / (1_f32 + 6_f32 * (resonance - 1_f32));

            // A Q of zero or below puts the poles outside the unit circle, which makes
            // the output grow with every sample instead of decaying. SoundFonts can ask
            // for it through initial attenuation and a filter Q below -1 dB, so such a
            // filter is treated as having no resonance at all.
            // https://github.com/sinshu/rustysynth/issues/57
            if q.is_nan() || q <= 0_f32 {
                self.active = false;

                return;
            }

            let w = 2_f32 * consts::PI * cutoff_frequency / self.sample_rate as f32;
            let cosw = w.cos();
            let alpha = w.sin() / (2_f32 * q);

            let b0 = (1_f32 - cosw) / 2_f32;
            let b1 = 1_f32 - cosw;
            let b2 = (1_f32 - cosw) / 2_f32;
            let a0 = 1_f32 + alpha;
            let a1 = -2_f32 * cosw;
            let a2 = 1_f32 - alpha;

            self.set_coefficients(a0, a1, a2, b0, b1, b2);
        } else {
            self.active = false;
        }
    }

    pub(crate) fn process(&mut self, block: &mut [f32]) {
        let block_length = block.len();

        if self.active {
            // Hoist the coefficients and the filter state into locals so the loop
            // body does not reload them from memory on every sample.
            let a0 = self.a0;
            let a1 = self.a1;
            let a2 = self.a2;
            let a3 = self.a3;
            let a4 = self.a4;
            let mut x1 = self.x1;
            let mut x2 = self.x2;
            let mut y1 = self.y1;
            let mut y2 = self.y2;

            for input in block.iter_mut() {
                let output = a0 * *input + a1 * x1 + a2 * x2 - a3 * y1 - a4 * y2;

                x2 = x1;
                x1 = *input;
                y2 = y1;
                y1 = output;

                *input = output;
            }

            self.x1 = x1;
            self.x2 = x2;
            self.y1 = y1;
            self.y2 = y2;
        } else {
            self.x2 = block[block_length - 2];
            self.x1 = block[block_length - 1];
            self.y2 = self.x2;
            self.y1 = self.x1;
        }
    }

    /// Processes the low-pass filters of four voices in lock step.
    ///
    /// A single filter is a chain of dependent multiply-adds: every output feeds
    /// the next sample, which leaves the multiply-add unit idle for most of the
    /// time it takes to produce a sample. Four independent filters interleaved
    /// sample by sample keep it busy. The arithmetic of each filter is exactly
    /// the arithmetic of `process`, so the results do not change.
    pub(crate) fn process_four(
        (f0, b0): (&mut BiQuadFilter, &mut [f32]),
        (f1, b1): (&mut BiQuadFilter, &mut [f32]),
        (f2, b2): (&mut BiQuadFilter, &mut [f32]),
        (f3, b3): (&mut BiQuadFilter, &mut [f32]),
    ) {
        let a0 = [f0.a0, f1.a0, f2.a0, f3.a0];
        let a1 = [f0.a1, f1.a1, f2.a1, f3.a1];
        let a2 = [f0.a2, f1.a2, f2.a2, f3.a2];
        let a3 = [f0.a3, f1.a3, f2.a3, f3.a3];
        let a4 = [f0.a4, f1.a4, f2.a4, f3.a4];
        let mut x1 = [f0.x1, f1.x1, f2.x1, f3.x1];
        let mut x2 = [f0.x2, f1.x2, f2.x2, f3.x2];
        let mut y1 = [f0.y1, f1.y1, f2.y1, f3.y1];
        let mut y2 = [f0.y2, f1.y2, f2.y2, f3.y2];

        let length = b0.len().min(b1.len()).min(b2.len()).min(b3.len());
        let b0 = &mut b0[..length];
        let b1 = &mut b1[..length];
        let b2 = &mut b2[..length];
        let b3 = &mut b3[..length];

        for t in 0..length {
            let x = [b0[t], b1[t], b2[t], b3[t]];

            let mut y = [0_f32; 4];
            for i in 0..4 {
                y[i] = a0[i] * x[i] + a1[i] * x1[i] + a2[i] * x2[i] - a3[i] * y1[i] - a4[i] * y2[i];

                x2[i] = x1[i];
                x1[i] = x[i];
                y2[i] = y1[i];
                y1[i] = y[i];
            }

            b0[t] = y[0];
            b1[t] = y[1];
            b2[t] = y[2];
            b3[t] = y[3];
        }

        f0.x1 = x1[0];
        f0.x2 = x2[0];
        f0.y1 = y1[0];
        f0.y2 = y2[0];
        f1.x1 = x1[1];
        f1.x2 = x2[1];
        f1.y1 = y1[1];
        f1.y2 = y2[1];
        f2.x1 = x1[2];
        f2.x2 = x2[2];
        f2.y1 = y1[2];
        f2.y2 = y2[2];
        f3.x1 = x1[3];
        f3.x2 = x2[3];
        f3.y1 = y1[3];
        f3.y2 = y2[3];
    }

    fn set_coefficients(&mut self, a0: f32, a1: f32, a2: f32, b0: f32, b1: f32, b2: f32) {
        self.a0 = b0 / a0;
        self.a1 = b1 / a0;
        self.a2 = b2 / a0;
        self.a3 = a1 / a0;
        self.a4 = a2 / a0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The resonance fit returns a negative Q for a resonance around -1.3 dB,
    // which put the poles outside the unit circle and made the output grow by
    // a factor of 1.5 per sample (#57). A non-positive Q means no resonance,
    // so the filter is bypassed instead.
    #[test]
    fn test_a_negative_q_does_not_make_the_filter_unstable() {
        let settings = SynthesizerSettings::new(44100);
        let mut filter = BiQuadFilter::new(&settings);

        let resonance = 10_f32.powf(0.05 * -1.3);
        filter.set_low_pass_filter(1000_f32, resonance);

        let mut block = vec![0.5_f32; 64];
        filter.process(&mut block);

        assert!(
            block.iter().all(|sample| *sample == 0.5_f32),
            "the filter changed a block while it should have been bypassed"
        );
    }

    #[test]
    fn test_a_normal_resonance_still_filters() {
        let settings = SynthesizerSettings::new(44100);
        let mut filter = BiQuadFilter::new(&settings);

        filter.set_low_pass_filter(1000_f32, 1_f32);

        let mut block = vec![0_f32; 64];
        block[0] = 1_f32;
        filter.process(&mut block);

        assert!(
            block.iter().skip(1).any(|sample| *sample > 0_f32),
            "the filter did not pass the impulse"
        );
    }
}
