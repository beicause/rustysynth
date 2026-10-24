//! A SoundFont (`.sf2` and, behind the `sf3` feature, `.sf3`) MIDI synthesizer
//! written in pure Rust.
//!
//! A [`SoundFont`] is loaded once and shared behind an `Arc`, and a [`Synthesizer`]
//! built from it renders the waveform into a pair of `f32` buffers, either played
//! directly with [`Synthesizer::note_on`] and friends or driven by a [`MidiFile`]
//! through a [`MidiFileSequencer`].
//!
//! ```no_run
//! use std::fs::File;
//! use std::sync::Arc;
//!
//! use rustysynth_ext::{SoundFont, Synthesizer, SynthesizerSettings};
//!
//! let mut sf2 = File::open("TimGM6mb.sf2").unwrap();
//! let sound_font = Arc::new(SoundFont::new(&mut sf2).unwrap());
//!
//! let settings = SynthesizerSettings::new(44100);
//! let mut synthesizer = Synthesizer::new(&sound_font, &settings).unwrap();
//!
//! synthesizer.note_on(0, 60, 100);
//!
//! let mut left = vec![0.0_f32; 44100];
//! let mut right = vec![0.0_f32; 44100];
//! synthesizer.render(&mut left, &mut right);
//! ```

mod error;

mod array_math;
mod binary_reader;
mod four_cc;
mod read_counter;

mod generator;
mod generator_type;
mod instrument;
mod instrument_info;
mod instrument_region;
mod loop_mode;
mod preset;
mod preset_info;
mod preset_region;
mod sample_header;
mod soundfont;
mod soundfont_info;
mod soundfont_math;
mod soundfont_parameters;
mod soundfont_sampledata;
#[cfg(feature = "sf3")]
mod soundfont_sf3;
mod soundfont_version;
mod zone;
mod zone_info;

mod bi_quad_filter;
mod channel;
mod envelope_stage;
mod lfo;
mod modulation_envelope;
mod oscillator;
mod region_ex;
mod region_pair;
mod synthesizer;
mod synthesizer_settings;
mod voice;
mod voice_collection;
mod volume_envelope;

mod midifile;
mod midifile_looptype;
mod midifile_sequencer;

mod chorus;
mod reverb;

#[cfg(test)]
mod test_font;

pub use self::error::MidiFileError;
pub use self::error::SoundFontError;
pub use self::error::SynthesizerError;
pub use self::instrument::Instrument;
pub use self::instrument_region::InstrumentRegion;
pub use self::loop_mode::LoopMode;
pub use self::midifile::MidiFile;
pub use self::midifile::MidiMessage;
pub use self::midifile_looptype::MidiFileLoopType;
pub use self::midifile_sequencer::MidiFileSequencer;
pub use self::preset::Preset;
pub use self::preset_region::PresetRegion;
pub use self::sample_header::SampleHeader;
pub use self::soundfont::SoundFont;
pub use self::soundfont_info::SoundFontInfo;
pub use self::soundfont_version::SoundFontVersion;
pub use self::synthesizer::Synthesizer;
pub use self::synthesizer_settings::SynthesizerSettings;
