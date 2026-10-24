//! In-memory SoundFont2 fixtures shared by the unit tests.
//!
//! The sample SoundFonts in `samples/` are downloaded separately, so tests
//! that only need a font that loads build one from these helpers instead.

/// The single sample contained in a synthesized test SoundFont.
#[derive(Clone)]
pub(crate) struct SampleSpec {
    pub(crate) data: Vec<i16>,
    pub(crate) start: i32,
    pub(crate) end: i32,
    pub(crate) start_loop: i32,
    pub(crate) end_loop: i32,
    pub(crate) modes: u16,
}

pub(crate) fn sample_spec() -> SampleSpec {
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

pub(crate) fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// Appends the pad byte that RIFF inserts after a chunk of an odd size.
pub(crate) fn padded_chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = chunk(id, body);

    if !body.len().is_multiple_of(2) {
        out.push(0);
    }

    out
}

pub(crate) fn list(form: &[u8; 4], sub_chunks: &[Vec<u8>]) -> Vec<u8> {
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
pub(crate) fn preset_info(name: &str, zone_start: u16) -> Vec<u8> {
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
pub(crate) fn instrument_info(name: &str, zone_start: u16) -> Vec<u8> {
    let mut out = name_field(name);
    out.extend_from_slice(&zone_start.to_le_bytes());
    out
}

// A pbag/ibag record: generator and modulator indexes.
pub(crate) fn zone_info(generator_index: u16) -> Vec<u8> {
    let mut out = generator_index.to_le_bytes().to_vec();
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

// A pgen/igen record: generator type and value.
pub(crate) fn generator(generator_type: u16, value: u16) -> Vec<u8> {
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

/// Builds the smallest SoundFont2 that loads successfully: one preset with one
/// instrument region that refers to the given sample.
///
/// Both zone lists start with a global zone (no generators) and end with their
/// terminator record, so the actual instrumentation lives in a single
/// non-global zone. The fields of the builder exist so that a test can make
/// exactly one part of the file wrong.
pub(crate) struct FontBuilder {
    pub(crate) sample: SampleSpec,
    pub(crate) info_chunks: Vec<Vec<u8>>,
    pub(crate) smpl_chunks: usize,
    pub(crate) sm24_bytes: Option<Vec<u8>>,
    pub(crate) pmod_bytes: Option<Vec<u8>>,
    /// Extra generators of the instrument zone, as (type, value) pairs.
    pub(crate) instrument_generators: Vec<(u16, u16)>,
    /// Zone start index of the terminating preset record.
    pub(crate) preset_zone_end: u16,
    /// Zone start index of the terminating instrument record.
    pub(crate) instrument_zone_end: u16,
}

impl Default for FontBuilder {
    fn default() -> Self {
        Self {
            sample: sample_spec(),
            info_chunks: vec![chunk(b"ifil", &[2, 0, 0, 0])],
            smpl_chunks: 1,
            sm24_bytes: None,
            pmod_bytes: None,
            instrument_generators: Vec::new(),
            preset_zone_end: 2,
            instrument_zone_end: 2,
        }
    }
}

impl FontBuilder {
    pub(crate) fn build(&self) -> Vec<u8> {
        let sample_bytes: Vec<u8> = self
            .sample
            .data
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();

        let info = list(b"INFO", &self.info_chunks);

        let mut sdta_chunks: Vec<Vec<u8>> =
            vec![padded_chunk(b"smpl", &sample_bytes); self.smpl_chunks];
        if let Some(bytes) = &self.sm24_bytes {
            sdta_chunks.push(padded_chunk(b"sm24", bytes));
        }
        let sdta = list(b"sdta", &sdta_chunks);

        let mut pdta_chunks = vec![
            chunk(
                b"phdr",
                &[
                    preset_info("test_preset", 0),
                    preset_info("", self.preset_zone_end),
                ]
                .concat(),
            ),
            chunk(
                b"pbag",
                &[zone_info(0), zone_info(0), zone_info(1)].concat(),
            ),
        ];

        if let Some(bytes) = &self.pmod_bytes {
            pdta_chunks.push(padded_chunk(b"pmod", bytes));
        }

        pdta_chunks.push(chunk(
            b"pgen",
            &[generator(41, 0), generator(0, 0)].concat(),
        ));
        pdta_chunks.push(chunk(
            b"inst",
            &[
                instrument_info("test_instrument", 0),
                instrument_info("", self.instrument_zone_end),
            ]
            .concat(),
        ));
        // The span of the tone zone runs up to the terminating zone record, so
        // it covers the two fixed generators plus whatever the test added.
        let instrument_zone_length = 2 + self.instrument_generators.len() as u16;
        pdta_chunks.push(chunk(
            b"ibag",
            &[
                zone_info(0),
                zone_info(0),
                zone_info(instrument_zone_length),
            ]
            .concat(),
        ));

        let mut igen_records = vec![
            generator(53, 0),                 // sample id
            generator(54, self.sample.modes), // sample modes
        ];
        for (generator_type, value) in &self.instrument_generators {
            igen_records.push(generator(*generator_type, *value));
        }
        igen_records.push(generator(0, 0)); // terminator
        pdta_chunks.push(chunk(b"igen", &igen_records.concat()));

        pdta_chunks.push(chunk(
            b"shdr",
            &[sample_header(&self.sample), vec![0_u8; 46]].concat(),
        ));

        let pdta = list(b"pdta", &pdta_chunks);

        let mut body = b"sfbk".to_vec();
        body.extend_from_slice(&info);
        body.extend_from_slice(&sdta);
        body.extend_from_slice(&pdta);

        chunk(b"RIFF", &body)
    }
}

pub(crate) fn build_sf2(sample: &SampleSpec, smpl_chunks: usize) -> Vec<u8> {
    FontBuilder {
        sample: sample.clone(),
        smpl_chunks,
        ..FontBuilder::default()
    }
    .build()
}
