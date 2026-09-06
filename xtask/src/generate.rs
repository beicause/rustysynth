//! `cargo xtask generate-goldens` - regenerates the `fluidr3mono_*`,
//! `fluidr3mono_sf3_*` and `timgm6mb_*` golden-value test data in
//! `rustysynth_test/` from the corresponding SoundFont files in `samples/`
//! (FluidR3Mono_GM.sf2 / .sf3, TimGM6mb.sf2).
//!
//! For each font it outputs:
//!
//! * `data/<prefix>_sample_headers.bin`     - one `[i32; 7]` row per sample
//!   (start, end, start_loop, end_loop, sample_rate, original_pitch,
//!   pitch_correction), little-endian. For the .sf3 font the offsets are the
//!   decoded 16-bit PCM coordinates produced by `SoundFont::new`, so the
//!   snapshot also pins the decompressed sample lengths.
//! * `data/<prefix>_instrument_regions.bin` - one `[f64; 50]` row per
//!   instrument region, little-endian, in the field order used by
//!   `instrument_util::check`.
//! * `data/<prefix>_preset_regions.bin`     - one `[f64; 39]` row per preset
//!   region, little-endian, in the field order used by `preset_util::check`.
//! * `src/<prefix>_{info,sample,instrument,preset}_test.rs` - the tests, which
//!   read the binary data at runtime with `std::fs`. The `.sf3` snapshot test
//!   modules are gated behind rustysynth_test's `sf3` feature in lib.rs.
//!
//! Keeping the golden values in binary files instead of Rust source keeps the
//! test compilation fast: rustc only parses the small test functions, while
//! the bulk of the data lives outside the build.
//!
//! Run this task and commit the result whenever the golden data needs to be
//! refreshed (e.g. after fixing a loading bug that legitimately changes the
//! parsed values, or when swapping the source font).

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use rustysynth_ext::{LoopMode, SoundFont};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/xtask
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Field order must stay in sync with `instrument_util::check`.
fn instrument_region_row(region: &rustysynth_ext::InstrumentRegion) -> Vec<f64> {
    let loop_mode = match region.get_sample_modes() {
        LoopMode::NoLoop => 0.0,
        LoopMode::Continuous => 1.0,
        LoopMode::LoopUntilNoteOff => 3.0,
    };
    vec![
        region.get_sample_start() as f64,
        region.get_sample_end() as f64,
        region.get_sample_start_loop() as f64,
        region.get_sample_end_loop() as f64,
        region.get_start_address_offset() as f64,
        region.get_end_address_offset() as f64,
        region.get_start_loop_address_offset() as f64,
        region.get_end_loop_address_offset() as f64,
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
        loop_mode,
        region.get_scale_tuning() as f64,
        region.get_exclusive_class() as f64,
        region.get_root_key() as f64,
    ]
}

/// Field order must stay in sync with `preset_util::check`.
fn preset_region_row(region: &rustysynth_ext::PresetRegion) -> Vec<f64> {
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

fn write_and_format(path: &Path, content: &str) {
    fs::write(path, content).expect("failed to write golden test file");
    let status = std::process::Command::new("rustfmt")
        .arg(path)
        .status()
        .unwrap_or_else(|err| panic!("failed to run rustfmt on {}: {err}", path.display()));
    assert!(status.success(), "rustfmt failed on {}", path.display());
}

fn write_f64_rows(path: &Path, rows: &[Vec<f64>]) {
    let mut buf = Vec::new();
    for row in rows {
        for value in row {
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
    fs::write(path, buf).unwrap();
}

/// Common file header; `util` is the check helper to import ("" for the info
/// test) and `test_fn` the test function name.
fn common_header(font_file: &str, util: &str, test_fn: &str) -> String {
    let mut s = String::new();
    s.push_str("#![allow(unused_imports, reason = \"generated golden test\")]\n\n");
    s.push_str("// Generated by `cargo xtask generate-goldens`.\n");
    s.push_str(&format!(
        "// From samples/{font_file}. Do not edit by hand.\n\n"
    ));
    s.push_str("use rustysynth_ext::SoundFont;\n");
    s.push_str("use std::fs::{self, File};\n");
    s.push_str("use std::path::PathBuf;\n\n");
    if !util.is_empty() {
        s.push_str(&format!("use crate::{util};\n\n"));
    }
    s.push_str("#[test]\n");
    s.push_str(&format!("fn {test_fn}() {{\n"));
    s.push_str("    let mut path = PathBuf::from(env!(\"CARGO_MANIFEST_DIR\"));\n");
    s.push_str("    path.pop();\n");
    s.push_str("    path.push(\"samples\");\n");
    s.push_str(&format!("    path.push(\"{font_file}\");\n"));
    s.push_str("    let mut file = File::open(&path).unwrap();\n");
    s.push_str("    let sf = SoundFont::new(&mut file).unwrap();\n\n");
    s
}

fn emit_info_test(src: &Path, prefix: &str, font_file: &str, sf: &SoundFont, tolerant_sum: bool) {
    let mut s = String::new();
    s.push_str(&common_header(font_file, "", "soundfont_info"));
    let info = sf.get_info();
    s.push_str("    let info = sf.get_info();\n\n");
    s.push_str(&format!(
        "    assert_eq!(info.get_version().get_major(), {});\n",
        info.get_version().get_major()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_version().get_minor(), {});\n",
        info.get_version().get_minor()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_target_sound_engine(), {:?});\n",
        info.get_target_sound_engine()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_bank_name(), {:?});\n",
        info.get_bank_name()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_rom_name(), {:?});\n",
        info.get_rom_name()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_rom_version().get_major(), {});\n",
        info.get_rom_version().get_major()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_rom_version().get_minor(), {});\n",
        info.get_rom_version().get_minor()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_creation_date(), {:?});\n",
        info.get_creation_date()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_author(), {:?});\n",
        info.get_author()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_target_product(), {:?});\n",
        info.get_target_product()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_copyright(), {:?});\n",
        info.get_copyright()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_comments().len(), {});\n",
        info.get_comments().len()
    ));
    s.push_str(&format!(
        "    assert_eq!(info.get_tools(), {:?});\n",
        info.get_tools()
    ));
    s.push('\n');
    s.push_str(&format!(
        "    assert_eq!(sf.get_wave_data().len(), {});\n\n",
        sf.get_wave_data().len()
    ));
    s.push_str("    let mut sum: i32 = 0;\n");
    s.push_str("    for value in sf.get_wave_data().iter() {\n");
    s.push_str("        sum += *value as i32;\n");
    s.push_str("    }\n");
    let expected_sum = sf.get_wave_data().iter().map(|v| *v as i64).sum::<i64>();
    if tolerant_sum {
        // SoundFont3 PCM is produced by the Ogg Vorbis decoder, whose
        // floating-point math differs slightly between toolchains (MSVC,
        // clang, gcc), so the exact sum is not reproducible everywhere.
        // Keep a relative tolerance that still catches corrupted decodes.
        let bound = expected_sum / 10_000;
        s.push_str(&format!(
            "    assert!(\n        (sum as i64 - {expected_sum}).abs() <= {bound},\n        \"decoded PCM sum ({{sum}}) differs from the snapshot by more than 0.01%\"\n    );"
        ));
    } else {
        s.push_str(&format!("    assert_eq!(sum, {expected_sum})"));
    }
    s.push_str("\n}\n");
    write_and_format(&src.join(format!("{prefix}_info_test.rs")), &s);
}

fn emit_sample_test(src: &Path, prefix: &str, font_file: &str, sf: &SoundFont) {
    let sample_headers = sf.get_sample_headers();
    let n = sample_headers.len();
    let mut s = String::new();
    s.push_str(&common_header(font_file, "sample_util", "samples"));
    s.push_str(&format!(
        "    assert_eq!(sf.get_sample_headers().len(), {n});\n\n"
    ));
    s.push_str(&format!(
        "    let data_path = PathBuf::from(env!(\"CARGO_MANIFEST_DIR\")).join(\"data/{prefix}_sample_headers.bin\");\n"
    ));
    s.push_str("    let data = fs::read(&data_path).unwrap();\n");
    s.push_str(&format!(
        "    assert_eq!(data.len(), {n} * 7 * {});\n\n",
        std::mem::size_of::<i32>()
    ));
    s.push_str("    for (index, sample) in sf.get_sample_headers().iter().enumerate() {\n");
    s.push_str("        let row = &data[index * 7 * 4..(index + 1) * 7 * 4];\n");
    s.push_str("        let values: [i32; 7] = std::array::from_fn(|i| {\n");
    s.push_str("            i32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap())\n");
    s.push_str("        });\n");
    s.push_str("        sample_util::check(sample, &values);\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    write_and_format(&src.join(format!("{prefix}_sample_test.rs")), &s);
}

fn emit_instrument_test(src: &Path, prefix: &str, font_file: &str, sf: &SoundFont) {
    let instruments = sf.get_instruments();
    let region_count: usize = instruments.iter().map(|i| i.get_regions().len()).sum();
    let mut s = String::new();
    s.push_str(&common_header(font_file, "instrument_util", "regions"));
    s.push_str(&format!(
        "    assert_eq!(sf.get_instruments().len(), {});\n",
        instruments.len()
    ));
    s.push_str(&format!(
        "    assert_eq!(sf.get_instruments().iter().map(|i| i.get_regions().len()).sum::<usize>(), {region_count});\n\n"
    ));
    s.push_str(&format!(
        "    let data_path = PathBuf::from(env!(\"CARGO_MANIFEST_DIR\")).join(\"data/{prefix}_instrument_regions.bin\");\n"
    ));
    s.push_str("    let data = fs::read(&data_path).unwrap();\n");
    s.push_str(&format!(
        "    assert_eq!(data.len(), {region_count} * 50 * {});\n\n",
        std::mem::size_of::<f64>()
    ));
    s.push_str("    let mut index = 0;\n");
    s.push_str("    for instrument in sf.get_instruments() {\n");
    s.push_str("        for region in instrument.get_regions() {\n");
    s.push_str("            let row = &data[index * 50 * 8..(index + 1) * 50 * 8];\n");
    s.push_str("            let values: [f64; 50] = std::array::from_fn(|i| {\n");
    s.push_str("                f64::from_le_bytes(row[i * 8..i * 8 + 8].try_into().unwrap())\n");
    s.push_str("            });\n");
    s.push_str("            instrument_util::check(region, &values);\n");
    s.push_str("            index += 1;\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    write_and_format(&src.join(format!("{prefix}_instrument_test.rs")), &s);
}

fn emit_preset_test(src: &Path, prefix: &str, font_file: &str, sf: &SoundFont) {
    let presets = sf.get_presets();
    let region_count: usize = presets.iter().map(|p| p.get_regions().len()).sum();
    let mut s = String::new();
    s.push_str(&common_header(font_file, "preset_util", "regions"));
    s.push_str(&format!(
        "    assert_eq!(sf.get_presets().len(), {});\n",
        presets.len()
    ));
    s.push_str(&format!(
        "    assert_eq!(sf.get_presets().iter().map(|p| p.get_regions().len()).sum::<usize>(), {region_count});\n\n"
    ));
    s.push_str(&format!(
        "    let data_path = PathBuf::from(env!(\"CARGO_MANIFEST_DIR\")).join(\"data/{prefix}_preset_regions.bin\");\n"
    ));
    s.push_str("    let data = fs::read(&data_path).unwrap();\n");
    s.push_str(&format!(
        "    assert_eq!(data.len(), {region_count} * 39 * {});\n\n",
        std::mem::size_of::<f64>()
    ));
    s.push_str("    let mut index = 0;\n");
    s.push_str("    for preset in sf.get_presets() {\n");
    s.push_str("        for region in preset.get_regions() {\n");
    s.push_str("            let row = &data[index * 39 * 8..(index + 1) * 39 * 8];\n");
    s.push_str("            let values: [f64; 39] = std::array::from_fn(|i| {\n");
    s.push_str("                f64::from_le_bytes(row[i * 8..i * 8 + 8].try_into().unwrap())\n");
    s.push_str("            });\n");
    s.push_str("            preset_util::check(region, &values);\n");
    s.push_str("            index += 1;\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    write_and_format(&src.join(format!("{prefix}_preset_test.rs")), &s);
}

fn process(font_file: &str, prefix: &str) {
    let root = repo_root();
    let data_dir = root.join("rustysynth_test/data");
    let src = root.join("rustysynth_test/src");
    fs::create_dir_all(&data_dir).unwrap();

    let mut sf_file = File::open(root.join(format!("samples/{font_file}"))).unwrap();
    let sf = SoundFont::new(&mut sf_file).unwrap();

    // Sample headers: one [i32; 7] row per sample.
    let sample_headers = sf.get_sample_headers();
    let mut sample_buf = Vec::new();
    for sample in sample_headers {
        for value in [
            sample.get_start(),
            sample.get_end(),
            sample.get_start_loop(),
            sample.get_end_loop(),
            sample.get_sample_rate(),
            sample.get_original_pitch(),
            sample.get_pitch_correction(),
        ] {
            sample_buf.extend_from_slice(&value.to_le_bytes());
        }
    }
    fs::write(
        data_dir.join(format!("{prefix}_sample_headers.bin")),
        &sample_buf,
    )
    .unwrap();

    // Instrument regions: one [f64; 50] row per region.
    let instruments = sf.get_instruments();
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for instrument in instruments {
        for region in instrument.get_regions() {
            rows.push(instrument_region_row(region));
        }
    }
    write_f64_rows(
        &data_dir.join(format!("{prefix}_instrument_regions.bin")),
        &rows,
    );

    // Preset regions: one [f64; 39] row per region.
    let presets = sf.get_presets();
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for preset in presets {
        for region in preset.get_regions() {
            rows.push(preset_region_row(region));
        }
    }
    write_f64_rows(
        &data_dir.join(format!("{prefix}_preset_regions.bin")),
        &rows,
    );

    // The decoded PCM of a SoundFont3 is not bit-exact across compilers,
    // so its sum assertion uses a relative tolerance instead of equality.
    emit_info_test(&src, prefix, font_file, &sf, prefix == "fluidr3mono_sf3");
    emit_sample_test(&src, prefix, font_file, &sf);
    emit_instrument_test(&src, prefix, font_file, &sf);
    emit_preset_test(&src, prefix, font_file, &sf);

    println!(
        "{prefix}: samples={} instruments={} presets={}",
        sample_headers.len(),
        instruments.len(),
        presets.len()
    );
}

pub fn run() -> i32 {
    process("FluidR3Mono_GM.sf2", "fluidr3mono");
    process("FluidR3Mono_GM.sf3", "fluidr3mono_sf3");
    process("TimGM6mb.sf2", "timgm6mb");
    0
}
