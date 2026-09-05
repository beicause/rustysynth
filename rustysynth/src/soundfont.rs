#![allow(dead_code)]

use std::io::Read;

use crate::binary_reader::BinaryReader;
use crate::error::SoundFontError;
use crate::four_cc::FourCC;
use crate::instrument::Instrument;
use crate::preset::Preset;
use crate::sample_header::SampleHeader;
use crate::soundfont_info::SoundFontInfo;
use crate::soundfont_parameters::SoundFontParameters;
use crate::soundfont_sampledata::SoundFontSampleData;
use crate::LoopMode;

/// Reperesents a SoundFont.
#[derive(Debug)]
#[non_exhaustive]
pub struct SoundFont {
    pub(crate) info: SoundFontInfo,
    pub(crate) bits_per_sample: i32,
    pub(crate) wave_data: Vec<i16>,
    pub(crate) sample_headers: Vec<SampleHeader>,
    pub(crate) presets: Vec<Preset>,
    pub(crate) instruments: Vec<Instrument>,
}

impl SoundFont {
    /// Loads a SoundFont from the stream.
    ///
    /// # Arguments
    ///
    /// * `reader` - The data stream used to load the SoundFont.
    pub fn new<R: Read>(reader: &mut R) -> Result<Self, SoundFontError> {
        let chunk_id = BinaryReader::read_four_cc(reader)?;
        if chunk_id != b"RIFF" {
            return Err(SoundFontError::RiffChunkNotFound);
        }

        let _size = BinaryReader::read_i32(reader)?;

        let form_type = BinaryReader::read_four_cc(reader)?;
        if form_type != b"sfbk" {
            return Err(SoundFontError::InvalidRiffChunkType {
                expected: FourCC::from_bytes(*b"sfbk"),
                actual: form_type,
            });
        }

        let info = SoundFontInfo::new(reader)?;
        let sample_bytes = SoundFontSampleData::read(reader)?;
        let (parameters, sample_data) = SoundFontParameters::load(reader, sample_bytes)?;

        let sound_font = Self {
            info,
            bits_per_sample: sample_data.bits_per_sample,
            wave_data: sample_data.wave_data,
            sample_headers: parameters.sample_headers,
            presets: parameters.presets,
            instruments: parameters.instruments,
        };

        sound_font.sanity_check()?;

        Ok(sound_font)
    }

    fn sanity_check(&self) -> Result<(), SoundFontError> {
        // https://github.com/sinshu/rustysynth/issues/22
        // https://github.com/sinshu/rustysynth/issues/33
        // https://github.com/sinshu/rustysynth/pull/51
        for instrument in &self.instruments {
            for region in &instrument.regions {
                let start = region.get_sample_start();
                let end = region.get_sample_end();
                let start_loop = region.get_sample_start_loop();
                let end_loop = region.get_sample_end_loop();
                let loop_mode = region.get_sample_modes();

                if start < 0
                    || start_loop < 0
                    || end as usize >= self.wave_data.len()
                    || end_loop as usize >= self.wave_data.len()
                    || end <= start
                    || end_loop < start_loop
                    || (loop_mode != LoopMode::NoLoop && start_loop >= end_loop)
                {
                    return Err(SoundFontError::SanityCheckFailed);
                }
            }
        }

        Ok(())
    }

    /// Gets the information of the SoundFont.
    pub fn get_info(&self) -> &SoundFontInfo {
        &self.info
    }

    /// Gets the bits per sample of the sample data.
    pub fn get_bits_per_sample(&self) -> i32 {
        self.bits_per_sample
    }

    /// Gets the sample data.
    pub fn get_wave_data(&self) -> &[i16] {
        &self.wave_data[..]
    }

    /// Gets the samples of the SoundFont.
    pub fn get_sample_headers(&self) -> &[SampleHeader] {
        &self.sample_headers[..]
    }

    /// Gets the presets of the SoundFont.
    pub fn get_presets(&self) -> &[Preset] {
        &self.presets[..]
    }

    /// Gets the instruments of the SoundFont.
    pub fn get_instruments(&self) -> &[Instrument] {
        &self.instruments[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{fs::File, path::PathBuf};

    fn samples_dir_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("samples")
    }

    #[cfg(not(feature = "sf3"))]
    #[test]
    fn test_load_reject_sf3() {
        let path = samples_dir_path().join("dummy.sf3");
        let mut file = File::open(&path).unwrap();
        assert!(matches!(
            SoundFont::new(&mut file),
            Err(SoundFontError::UnsupportedSampleFormat)
        ));
    }

    #[cfg(feature = "sf3")]
    #[test]
    fn test_load_sf3_dummy() {
        let path = samples_dir_path().join("dummy.sf3");
        let mut file = File::open(&path).unwrap();
        let sound_font = SoundFont::new(&mut file).unwrap();
        assert_eq!(sound_font.get_bits_per_sample(), 16);
        assert!(!sound_font.get_wave_data().is_empty());
        assert!(!sound_font.get_sample_headers().is_empty());
        assert!(!sound_font.get_presets().is_empty());
    }

    #[cfg(feature = "sf3")]
    #[test]
    fn test_load_sf3_fluid() {
        let path = samples_dir_path().join("FluidR3Mono_GM.sf3");
        let mut file = File::open(&path).unwrap();
        let sound_font = SoundFont::new(&mut file).unwrap();

        assert_eq!(sound_font.get_bits_per_sample(), 16);
        assert!(!sound_font.get_wave_data().is_empty());
        assert!(!sound_font.get_sample_headers().is_empty());
        assert!(!sound_font.get_presets().is_empty());
        assert!(!sound_font.get_instruments().is_empty());

        // Every decoded sample header must reference valid PCM data.
        let wave_len = sound_font.get_wave_data().len();
        for sample in sound_font.get_sample_headers() {
            assert!(sample.get_start() >= 0);
            assert!(sample.get_start() < sample.get_end());
            assert!(sample.get_end() as usize <= wave_len);
            assert!(sample.get_end_loop() as usize <= wave_len);
        }

        // Rendering a note must produce audible output; this guards against
        // decoding the samples after the instrument regions are built.
        use std::sync::Arc;

        use crate::{Synthesizer, SynthesizerSettings};

        let sound_font = Arc::new(sound_font);
        let settings = SynthesizerSettings::new(44100);
        let mut synthesizer = Synthesizer::new(&sound_font, &settings).unwrap();
        synthesizer.note_on(0, 60, 100); // grand piano, middle C

        let mut left = vec![0_f32; 44100];
        let mut right = vec![0_f32; 44100];
        synthesizer.render(&mut left, &mut right);

        let peak = left
            .iter()
            .fold(0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            peak > 0.01,
            "the rendered note was effectively silent (peak {peak})"
        );
    }

    // smpl sub-chunk exists, but is zero-length.
    #[test]
    fn test_load_empty_samples() {
        let path = samples_dir_path().join("test_empty_samples.sf2");
        let mut file = File::open(&path).unwrap();
        assert!(matches!(
            SoundFont::new(&mut file),
            Err(SoundFontError::SampleDataNotFound)
        ));
    }
}
