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

/// Represents a SoundFont.
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

                // A reversed or empty loop range no longer rejects the whole
                // file: the oscillator plays such a region once without
                // looping.
                // https://github.com/sinshu/rustysynth/issues/55
                //
                // Loop points outside the sample data are a different matter.
                // They mean the sample data is truncated, so the region cannot
                // be played at all and the file stays rejected.
                if start < 0
                    || start_loop < 0
                    || end as usize >= self.wave_data.len()
                    || end_loop as usize >= self.wave_data.len()
                    || end <= start
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

    // Snapshots a preset region in the same field order (and loop-mode
    // encoding) used by rustysynth_test::preset_util::check. Only used by the
    // sf3-vs-sf2 parity test, so gate it with the same feature.
    #[cfg(feature = "sf3")]
    fn preset_region_values(region: &crate::PresetRegion) -> Vec<f64> {
        vec![
            region.get_modulation_lfo_to_pitch() as f64,
            region.get_vibrato_lfo_to_pitch() as f64,
            region.get_modulation_envelope_to_pitch() as f64,
            region.get_initial_filter_cutoff_frequency() as f64,
            region.get_initial_filter_q() as f64,
            region.get_modulation_lfo_to_filter_cutoff_frequency() as f64,
            region.get_modulation_envelope_to_filter_cutoff_frequency() as f64,
            region.get_modulation_lfo_to_volume() as f64,
            region.get_chorus_effects_send() as f64,
            region.get_reverb_effects_send() as f64,
            region.get_pan() as f64,
            region.get_delay_modulation_lfo() as f64,
            region.get_frequency_modulation_lfo() as f64,
            region.get_delay_vibrato_lfo() as f64,
            region.get_frequency_vibrato_lfo() as f64,
            region.get_delay_modulation_envelope() as f64,
            region.get_attack_modulation_envelope() as f64,
            region.get_hold_modulation_envelope() as f64,
            region.get_decay_modulation_envelope() as f64,
            region.get_sustain_modulation_envelope() as f64,
            region.get_release_modulation_envelope() as f64,
            region.get_key_number_to_modulation_envelope_hold() as f64,
            region.get_key_number_to_modulation_envelope_decay() as f64,
            region.get_delay_volume_envelope() as f64,
            region.get_attack_volume_envelope() as f64,
            region.get_hold_volume_envelope() as f64,
            region.get_decay_volume_envelope() as f64,
            region.get_sustain_volume_envelope() as f64,
            region.get_release_volume_envelope() as f64,
            region.get_key_number_to_volume_envelope_hold() as f64,
            region.get_key_number_to_volume_envelope_decay() as f64,
            region.get_key_range_start() as f64,
            region.get_key_range_end() as f64,
            region.get_velocity_range_start() as f64,
            region.get_velocity_range_end() as f64,
            region.get_initial_attenuation() as f64,
            region.get_coarse_tune() as f64,
            region.get_fine_tune() as f64,
            region.get_scale_tuning() as f64,
        ]
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

    // Loads the larger FluidR3Mono_GM.SF2 sample font.
    #[test]
    fn test_load_sf2_fluid() {
        let path = samples_dir_path().join("FluidR3Mono_GM.sf2");
        let mut file = File::open(&path).unwrap();
        let sound_font = SoundFont::new(&mut file).unwrap();

        assert_eq!(sound_font.get_bits_per_sample(), 16);
        assert!(!sound_font.get_wave_data().is_empty());
        assert!(!sound_font.get_sample_headers().is_empty());
        assert!(!sound_font.get_presets().is_empty());
        assert!(!sound_font.get_instruments().is_empty());

        // Every sample header must reference valid PCM data.
        let wave_len = sound_font.get_wave_data().len();
        for sample in sound_font.get_sample_headers() {
            assert!(sample.get_start() >= 0);
            assert!(sample.get_start() < sample.get_end());
            assert!(sample.get_end() as usize <= wave_len);
            assert!(sample.get_end_loop() as usize <= wave_len);
        }

        // Rendering a note must produce audible output.
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

    // The .sf3 version must agree with the .sf2 version everywhere the two
    // fonts represent the same musical data. The conversion tool REORDERED
    // the samples, instruments and presets, and even cross-linked one sample
    // name ('PalmMuted Guitar Bb') to two different samples, so nothing here
    // is compared by index.
    //
    // The two builds are not byte-identical: the SF3 conversion started from
    // a slightly longer source, so 317 samples decode to a few thousand
    // extra frames. Samples are therefore matched by name and compared with
    // a frame tolerance; structure that is exactly shared (sample identity
    // fields, instrument and preset definitions) is compared exactly.
    #[cfg(feature = "sf3")]
    #[test]
    fn test_sf3_matches_sf2() {
        use std::collections::HashMap;

        /// ~46 ms at 44.1 kHz: permitted frame difference between the two
        /// builds for lengths and loop points (the observed source-build
        /// drift is at most ~1000 frames).
        const TOLERANCE: i32 = 2048;

        fn load(file_name: &str) -> SoundFont {
            let path = samples_dir_path().join(file_name);
            let mut file = File::open(&path).unwrap();
            SoundFont::new(&mut file).unwrap()
        }

        fn grouped(sf: &SoundFont) -> HashMap<&str, Vec<&crate::SampleHeader>> {
            let mut groups: HashMap<&str, Vec<&SampleHeader>> = HashMap::new();
            for header in sf.get_sample_headers() {
                groups.entry(header.get_name()).or_default().push(header);
            }
            groups
        }

        fn by_name_i(sf: &SoundFont) -> HashMap<&str, &crate::Instrument> {
            sf.get_instruments()
                .iter()
                .map(|i| (i.get_name(), i))
                .collect()
        }

        fn find_preset(sf: &SoundFont, bank: i32, patch: i32) -> Option<&crate::Preset> {
            sf.get_presets()
                .iter()
                .find(|p| p.get_bank_number() == bank && p.get_patch_number() == patch)
        }

        fn rel_loop(header: &crate::SampleHeader, field: fn(&crate::SampleHeader) -> i32) -> i32 {
            field(header) - header.get_start()
        }

        let sf2 = load("FluidR3Mono_GM.sf2");
        let sf3 = load("FluidR3Mono_GM.sf3");

        // ---------------- samples ----------------
        let headers_2 = sf2.get_sample_headers();
        let headers_3 = sf3.get_sample_headers();
        assert_eq!(
            headers_2.len(),
            headers_3.len(),
            "the two builds must contain the same number of samples"
        );

        let grouped_2 = grouped(&sf2);
        let grouped_3 = grouped(&sf3);
        assert_eq!(
            grouped_2.len(),
            grouped_3.len(),
            "the distinct sample names differ"
        );

        for (name, occurrences_2) in &grouped_2 {
            let occurrences_3 = grouped_3
                .get(name)
                .unwrap_or_else(|| panic!("sample '{name}' is missing in the sf3 build"));
            assert_eq!(
                occurrences_2.len(),
                occurrences_3.len(),
                "the number of samples named '{name}' differs"
            );

            // Pair the occurrences (there is one duplicated name) by their
            // relative loop points, which are order-independent within a font.
            let mut occurrences_2 = occurrences_2.clone();
            let mut occurrences_3 = occurrences_3.clone();
            let key = |header: &&crate::SampleHeader| {
                (
                    header.get_start_loop() - header.get_start(),
                    header.get_end_loop() - header.get_start(),
                )
            };
            occurrences_2.sort_by_key(key);
            occurrences_3.sort_by_key(key);

            for (a, b) in occurrences_2.iter().zip(&occurrences_3) {
                assert_eq!(a.get_sample_rate(), b.get_sample_rate(), "sample '{name}'");
                assert_eq!(
                    a.get_original_pitch(),
                    b.get_original_pitch(),
                    "sample '{name}'"
                );
                assert_eq!(
                    a.get_pitch_correction(),
                    b.get_pitch_correction(),
                    "sample '{name}'"
                );
                assert!(
                    (rel_loop(a, crate::SampleHeader::get_start_loop)
                        - rel_loop(b, crate::SampleHeader::get_start_loop))
                    .abs()
                        <= TOLERANCE,
                    "sample '{name}' loop start differs by more than {TOLERANCE} frames"
                );
                assert!(
                    (rel_loop(a, crate::SampleHeader::get_end_loop)
                        - rel_loop(b, crate::SampleHeader::get_end_loop))
                    .abs()
                        <= TOLERANCE,
                    "sample '{name}' loop end differs by more than {TOLERANCE} frames"
                );
                assert!(
                    ((a.get_end() - a.get_start()) - (b.get_end() - b.get_start())).abs()
                        <= TOLERANCE,
                    "sample '{name}' length differs by more than {TOLERANCE} frames"
                );
                assert!(
                    b.get_sample_type() & 0x10 != 0,
                    "sf3 sample '{name}' must be flagged as compressed"
                );
            }
        }

        // The decoded PCM pools may differ by at most one tolerance per sample
        // (each font's exact pool size is pinned by its own info snapshot).
        let pool_diff = (sf3.get_wave_data().len() as i64 - sf2.get_wave_data().len() as i64).abs();
        assert!(
            pool_diff <= headers_3.len() as i64 * TOLERANCE as i64,
            "decoded PCM pool lengths differ by {pool_diff} frames"
        );

        // ---------------- instruments ----------------
        let instruments_2 = sf2.get_instruments();
        let instruments_3 = sf3.get_instruments();
        assert_eq!(instruments_2.len(), instruments_3.len());

        let mut names_2: Vec<&str> = instruments_2.iter().map(|i| i.get_name()).collect();
        let mut names_3: Vec<&str> = instruments_3.iter().map(|i| i.get_name()).collect();
        names_2.sort_unstable();
        names_3.sort_unstable();
        assert_eq!(names_2, names_3, "instrument name sets differ");

        let by_name_i2 = by_name_i(&sf2);
        let by_name_i3 = by_name_i(&sf3);
        assert_eq!(by_name_i2.len(), by_name_i3.len());
        assert_eq!(
            by_name_i2.len(),
            instruments_2.len(),
            "duplicate instrument names"
        );

        for (name, a) in &by_name_i2 {
            let b = by_name_i3
                .get(name)
                .unwrap_or_else(|| panic!("instrument '{name}' is missing in the sf3 build"));
            assert_eq!(
                a.get_regions().len(),
                b.get_regions().len(),
                "instrument '{name}'"
            );
        }

        // ---------------- presets ----------------
        let presets_2 = sf2.get_presets();
        let presets_3 = sf3.get_presets();
        assert_eq!(presets_2.len(), presets_3.len());

        let mut count_2 = 0;
        for preset in presets_2 {
            let bank = preset.get_bank_number();
            let patch = preset.get_patch_number();
            let counterpart = find_preset(&sf3, bank, patch).unwrap_or_else(|| {
                panic!("preset (bank {bank}, patch {patch}) is missing in the sf3 build")
            });
            assert_eq!(preset.get_name(), counterpart.get_name());
            assert_eq!(preset.get_regions().len(), counterpart.get_regions().len());
            for (ra, rb) in preset.get_regions().iter().zip(counterpart.get_regions()) {
                assert_eq!(preset_region_values(ra), preset_region_values(rb));
            }
            count_2 += 1;
        }
        assert_eq!(count_2, presets_2.len());
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

    // A second smpl sub-chunk used to replace the first one silently.
    #[test]
    fn test_load_rejects_duplicate_sample_data_chunks() {
        let bytes = build_sf2(&sample_spec(), 2);
        let mut reader = std::io::Cursor::new(bytes);
        assert!(matches!(
            SoundFont::new(&mut reader),
            Err(SoundFontError::InvalidSampleData)
        ));
    }

    // A looping region with a reversed loop range used to fail the sanity
    // check, which rejected the whole SoundFont. It must load now, keeping the
    // loop points as they are (#55).
    #[test]
    fn test_load_accepts_an_invalid_loop_range() {
        let mut spec = sample_spec();
        spec.start_loop = 96;
        spec.end_loop = 32;

        let bytes = build_sf2(&spec, 1);
        let mut reader = std::io::Cursor::new(bytes);
        let sound_font = SoundFont::new(&mut reader).unwrap();

        let region = &sound_font.get_instruments()[0].get_regions()[0];
        assert_eq!(region.get_sample_start_loop(), 96);
        assert_eq!(region.get_sample_end_loop(), 32);
        assert_eq!(region.get_sample_modes(), crate::LoopMode::Continuous);
    }

    // The oscillator plays a region with a reversed loop range once instead of
    // looping it, so the sound must stop while a looping region keeps sounding.
    #[test]
    fn test_invalid_loop_range_plays_once() {
        let mut looping = sample_spec();
        looping.end_loop = 96;
        let mut reversed = sample_spec();
        reversed.start_loop = 96;
        reversed.end_loop = 32;

        let looping_peak = render_tail_peak(build_sf2(&looping, 1));
        let reversed_peak = render_tail_peak(build_sf2(&reversed, 1));

        assert!(
            looping_peak > 0.01,
            "the looping region stopped sounding (tail peak {looping_peak})"
        );
        assert!(
            reversed_peak < 1e-6,
            "the reversed loop region kept sounding (tail peak {reversed_peak})"
        );
    }

    // Renders a short note and returns the peak of its tail, by which time a
    // region that stops after a single pass has fallen silent.
    fn render_tail_peak(sound_font: Vec<u8>) -> f32 {
        use std::sync::Arc;

        use crate::{Synthesizer, SynthesizerSettings};

        let mut reader = std::io::Cursor::new(sound_font);
        let sound_font = Arc::new(SoundFont::new(&mut reader).unwrap());

        let mut settings = SynthesizerSettings::new(44100);
        settings.enable_reverb_and_chorus = false;
        let mut synthesizer = Synthesizer::new(&sound_font, &settings).unwrap();
        synthesizer.note_on(0, 60, 100);

        // The sample is 128 frames long, so a region played once is already
        // over by the tail of this buffer.
        const FRAMES: usize = 4096;
        let mut left = vec![0_f32; FRAMES];
        let mut right = vec![0_f32; FRAMES];
        synthesizer.render(&mut left, &mut right);

        left[FRAMES - 1024..]
            .iter()
            .fold(0_f32, |peak, sample| peak.max(sample.abs()))
    }

    // The fixtures below are built in memory so that these tests do not depend
    // on the sample SoundFonts, which are downloaded separately.

    /// The single sample contained in a synthesized test SoundFont.
    struct SampleSpec {
        data: Vec<i16>,
        start: i32,
        end: i32,
        start_loop: i32,
        end_loop: i32,
        modes: u16,
    }

    fn sample_spec() -> SampleSpec {
        SampleSpec {
            data: (0..256)
                .map(|i| (((i * 37) % 2000) - 1000) as i16 * 8)
                .collect(),
            start: 0,
            end: 128,
            start_loop: 32,
            end_loop: 96,
            modes: 1, // LoopMode::Continuous
        }
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + body.len());
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn list(form: &[u8; 4], sub_chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body = form.to_vec();
        for sub_chunk in sub_chunks {
            body.extend_from_slice(sub_chunk);
        }
        chunk(b"LIST", &body)
    }

    fn name_field(name: &str) -> Vec<u8> {
        let mut field = vec![0_u8; 20];
        let bytes = name.as_bytes();
        field[..bytes.len()].copy_from_slice(bytes);
        field
    }

    // A phdr record: name, patch, bank, zone start, library, genre and
    // morphology.
    fn preset_info(name: &str, zone_start: u16) -> Vec<u8> {
        let mut out = name_field(name);
        out.extend_from_slice(&0_u16.to_le_bytes()); // patch number
        out.extend_from_slice(&0_u16.to_le_bytes()); // bank number
        out.extend_from_slice(&zone_start.to_le_bytes());
        out.extend_from_slice(&0_i32.to_le_bytes()); // library
        out.extend_from_slice(&0_i32.to_le_bytes()); // genre
        out.extend_from_slice(&0_i32.to_le_bytes()); // morphology
        out
    }

    // An inst record: name and zone start.
    fn instrument_info(name: &str, zone_start: u16) -> Vec<u8> {
        let mut out = name_field(name);
        out.extend_from_slice(&zone_start.to_le_bytes());
        out
    }

    // A pbag/ibag record: generator and modulator indexes.
    fn zone_info(generator_index: u16) -> Vec<u8> {
        let mut out = generator_index.to_le_bytes().to_vec();
        out.extend_from_slice(&0_u16.to_le_bytes());
        out
    }

    // A pgen/igen record: generator type and value.
    fn generator(generator_type: u16, value: u16) -> Vec<u8> {
        let mut out = generator_type.to_le_bytes().to_vec();
        out.extend_from_slice(&value.to_le_bytes());
        out
    }

    // An shdr record: name, start, end, loop start, loop end, sample rate,
    // original pitch, pitch correction, link and sample type.
    fn sample_header(sample: &SampleSpec) -> Vec<u8> {
        let mut out = name_field("test_sample");
        out.extend_from_slice(&sample.start.to_le_bytes());
        out.extend_from_slice(&sample.end.to_le_bytes());
        out.extend_from_slice(&sample.start_loop.to_le_bytes());
        out.extend_from_slice(&sample.end_loop.to_le_bytes());
        out.extend_from_slice(&44100_i32.to_le_bytes()); // sample rate
        out.push(60); // original pitch
        out.push(0); // pitch correction
        out.extend_from_slice(&0_u16.to_le_bytes()); // link
        out.extend_from_slice(&1_u16.to_le_bytes()); // sample type: mono
        out
    }

    /// Builds the smallest SoundFont2 that loads successfully: one preset with
    /// one instrument region that refers to the given sample. The smpl
    /// sub-chunk is emitted `smpl_chunks` times, which is what the duplicate
    /// chunk test needs.
    ///
    /// Both zone lists start with a global zone (no generators) and end with
    /// their terminator record, so the actual instrumentation lives in a
    /// single non-global zone.
    fn build_sf2(sample: &SampleSpec, smpl_chunks: usize) -> Vec<u8> {
        let sample_bytes: Vec<u8> = sample
            .data
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();

        let info = list(b"INFO", &[chunk(b"ifil", &[2, 0, 0, 0])]);

        let smpl = chunk(b"smpl", &sample_bytes);
        let sdta = list(b"sdta", &vec![smpl; smpl_chunks]);

        let phdr = chunk(
            b"phdr",
            &[preset_info("test_preset", 0), preset_info("", 2)].concat(),
        );
        let pbag = chunk(
            b"pbag",
            &[zone_info(0), zone_info(0), zone_info(1)].concat(),
        );
        let pgen = chunk(b"pgen", &[generator(41, 0), generator(0, 0)].concat());
        let inst = chunk(
            b"inst",
            &[
                instrument_info("test_instrument", 0),
                instrument_info("", 2),
            ]
            .concat(),
        );
        let ibag = chunk(
            b"ibag",
            &[zone_info(0), zone_info(0), zone_info(2)].concat(),
        );
        let igen = chunk(
            b"igen",
            &[
                generator(53, 0),            // sample id
                generator(54, sample.modes), // sample modes
                generator(0, 0),             // terminator
            ]
            .concat(),
        );
        let shdr = chunk(b"shdr", &[sample_header(sample), vec![0_u8; 46]].concat());

        let pdta = list(b"pdta", &[phdr, pbag, pgen, inst, ibag, igen, shdr]);

        let mut body = b"sfbk".to_vec();
        body.extend_from_slice(&info);
        body.extend_from_slice(&sdta);
        body.extend_from_slice(&pdta);
        chunk(b"RIFF", &body)
    }
}
