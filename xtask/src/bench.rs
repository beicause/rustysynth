//! `cargo xtask bench` - benchmarks the SoundFont loader and the synthesizer.
//!
//! The reported number is the *CPU time* of the process, not the wall clock.
//! On a machine that is busy with something else the wall clock of a benchmark
//! swings by well over 10%, while the CPU time of a single-threaded workload is
//! reproducible to about 1% - which is the resolution needed to judge the kind
//! of change this project makes. The wall clock is still printed next to it,
//! because it is what a real-time user actually experiences.
//!
//! The CPU time comes from `/proc/self/stat` and is therefore only available on
//! Linux; elsewhere the wall clock is all there is. Its resolution is one clock
//! tick (10 ms), so every measurement is repeated until a single sample lasts
//! at least [`MIN_SAMPLE_SECONDS`] and the per-run cost is averaged out of it.
//!
//! The loader benchmarks need the fonts from `cargo xtask fetch-fonts`. The
//! MIDI benchmark builds its own sequence in memory, so it needs no assets.
//!
//! Run it in release mode, e.g. `cargo xtask-bench`.

use std::fmt;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rustysynth_ext::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};

/// The sample rate every benchmark runs at.
const SAMPLE_RATE: i32 = 44100;

/// Fonts under `samples/` to benchmark the loader with, in the order they are
/// reported. Missing ones are skipped with a hint.
const FONT_NAMES: [&str; 3] = ["FluidR3Mono_GM.sf2", "FluidR3Mono_GM.sf3", "TimGM6mb.sf2"];

/// How long a single sample must last before it is trusted, in seconds. One
/// clock tick is 10 ms, so this keeps the quantisation below 4%.
const MIN_SAMPLE_SECONDS: f64 = 0.3;

/// The block size the MIDI benchmark renders in, in samples. Similar to the
/// buffer of a real-time audio callback.
const RENDER_BLOCK: usize = 4096;

/// Ticks per quarter note of the generated MIDI file.
const TICKS_PER_QUARTER: u16 = 96;

/// Why a benchmark could not run at all.
#[derive(Debug)]
enum BenchError {
    Io(io::Error),
    Midi(rustysynth_ext::MidiFileError),
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchError::Io(err) => write!(f, "{err}"),
            BenchError::Midi(err) => write!(f, "cannot parse the generated MIDI file: {err}"),
        }
    }
}

impl From<io::Error> for BenchError {
    fn from(err: io::Error) -> Self {
        BenchError::Io(err)
    }
}

impl From<rustysynth_ext::MidiFileError> for BenchError {
    fn from(err: rustysynth_ext::MidiFileError) -> Self {
        BenchError::Midi(err)
    }
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/xtask
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The CPU time (user + system) this process has used so far, in seconds.
///
/// `None` where `/proc/self/stat` is unavailable, i.e. not on Linux.
fn cpu_seconds() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;

    // Field 2 is the command name, wrapped in parentheses and possibly
    // containing spaces, so parsing resumes after the closing one. Fields 14
    // and 15 (utime and stime) are then at index 11 and 12 of the remainder,
    // since the remainder starts at field 3.
    let rest = &stat[stat.rfind(')')? + 2..];
    let mut fields = rest.split_whitespace();
    let utime: f64 = fields.nth(11)?.parse().ok()?;
    let stime: f64 = fields.next()?.parse().ok()?;

    // USER_HZ is 100 on every Linux architecture that matters here.
    Some((utime + stime) / 100.0)
}

/// The fastest of several runs, from two clocks.
#[derive(Clone, Copy)]
struct Timing {
    /// The CPU time of one run, if the CPU clock was available.
    cpu: Option<f64>,
    /// The wall clock time of one run.
    wall: f64,
}

/// Measures `work`, keeping the fastest run of each clock.
///
/// `work` is calibrated once and then repeated enough times per sample to make
/// that sample last at least `min_seconds`, so that the 10 ms CPU clock is not
/// the dominant source of noise.
fn measure(reps: usize, min_seconds: f64, mut work: impl FnMut()) -> Timing {
    let calibration = Instant::now();
    work();
    let first = calibration.elapsed().as_secs_f64().max(1e-9);
    let runs = ((min_seconds / first).ceil() as usize).clamp(1, 100_000);

    let mut best = Timing {
        cpu: None,
        wall: f64::MAX,
    };
    for _ in 0..reps {
        let wall_start = Instant::now();
        let cpu_start = cpu_seconds();
        for _ in 0..runs {
            work();
        }
        let wall = wall_start.elapsed().as_secs_f64() / runs as f64;
        let cpu = match (cpu_start, cpu_seconds()) {
            (Some(start), Some(end)) => Some((end - start) / runs as f64),
            _ => None,
        };

        best.wall = best.wall.min(wall);
        if let Some(cpu) = cpu {
            best.cpu = Some(best.cpu.map_or(cpu, |old| old.min(cpu)));
        }
    }
    best
}

fn report(label: &str, timing: Timing, reps: usize) {
    match timing.cpu {
        Some(cpu) => println!(
            "{label:<30} cpu={cpu:7.4}s wall={:7.4}s  (best of {reps})",
            timing.wall
        ),
        None => println!(
            "{label:<30} wall={:7.4}s  (best of {reps}; the CPU clock needs Linux)",
            timing.wall
        ),
    }
}

fn missing_font(name: &str) {
    println!("skip: samples/{name} is missing, run `cargo xtask fetch-fonts`");
}

fn bench_load(samples: &Path, name: &str, reps: usize) {
    let path = samples.join(name);
    if !path.is_file() {
        missing_font(name);
        return;
    }

    let load = || {
        let mut file = File::open(&path).expect("the font exists, but cannot be opened");
        let sound_font = SoundFont::new(&mut file).expect("the font exists, but cannot be loaded");
        // Touch the decoded samples so that the whole chunk is read.
        std::hint::black_box(sound_font.get_wave_data().len());
    };
    let timing = measure(reps, MIN_SAMPLE_SECONDS, load);
    report(&format!("load/{name}"), timing, reps);
}

/// Builds a format-0 MIDI file of four channels, each playing a four note chord
/// per beat. The chords sit in different registers so that the voices run at a
/// spread of pitch ratios rather than all at the native sample rate.
fn build_midi(chords: u32) -> Vec<u8> {
    const ROOTS: [i32; 4] = [36, 48, 60, 72];
    const VELOCITY: u8 = 100;
    const RELEASE_VELOCITY: u8 = 64;

    let ticks = u32::from(TICKS_PER_QUARTER);
    let mut track: Vec<u8> = Vec::new();

    for step in 0..chords {
        // Only the first event of the whole track carries a delta for step 0.
        let mut delta = if step == 0 { 0 } else { ticks };

        // Release the chord of the previous beat.
        if step > 0 {
            for (channel, &root) in ROOTS.iter().enumerate() {
                for k in 0..4 {
                    write_variable_length(&mut track, delta);
                    delta = 0;
                    let key = (root + k * 2) as u8;
                    track.extend_from_slice(&[0x80 | channel as u8, key, RELEASE_VELOCITY]);
                }
            }
        }

        // Start the chord of this beat.
        for (channel, &root) in ROOTS.iter().enumerate() {
            for k in 0..4 {
                write_variable_length(&mut track, delta);
                delta = 0;
                let key = (root + k * 2) as u8;
                track.extend_from_slice(&[0x90 | channel as u8, key, VELOCITY]);
            }
        }
    }

    // Release the last chord, then end the track.
    let mut delta = ticks;
    for (channel, &root) in ROOTS.iter().enumerate() {
        for k in 0..4 {
            write_variable_length(&mut track, delta);
            delta = 0;
            let key = (root + k * 2) as u8;
            track.extend_from_slice(&[0x80 | channel as u8, key, RELEASE_VELOCITY]);
        }
    }
    write_variable_length(&mut track, 0);
    track.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    // Header chunk: format 0, one track, 96 ticks per quarter note.
    let mut file = Vec::with_capacity(track.len() + 22);
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6_u32.to_be_bytes());
    file.extend_from_slice(&0_u16.to_be_bytes());
    file.extend_from_slice(&1_u16.to_be_bytes());
    file.extend_from_slice(&TICKS_PER_QUARTER.to_be_bytes());
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&(track.len() as u32).to_be_bytes());
    file.extend_from_slice(&track);
    file
}

/// Appends a MIDI variable-length quantity, seven bits per byte, most
/// significant group first.
fn write_variable_length(buffer: &mut Vec<u8>, mut value: u32) {
    let mut groups = [0_u8; 4];
    let mut count = 0;
    loop {
        groups[count] = (value & 0x7F) as u8;
        count += 1;
        value >>= 7;
        if value == 0 {
            break;
        }
    }
    for index in (0..count).rev() {
        let mut byte = groups[index];
        if index != 0 {
            byte |= 0x80;
        }
        buffer.push(byte);
    }
}

/// Renders `total` samples of the sequence in blocks, appending to `left` and
/// `right` when they are given.
fn render_sequence(
    sound_font: &Arc<SoundFont>,
    midi: &Arc<MidiFile>,
    total: usize,
    mut sink: Option<(&mut Vec<f32>, &mut Vec<f32>)>,
) {
    let settings = SynthesizerSettings::new(SAMPLE_RATE);
    let synthesizer = Synthesizer::new(sound_font, &settings).unwrap();
    let mut sequencer = MidiFileSequencer::new(synthesizer);
    sequencer.play(midi, false);

    let mut left = [0_f32; RENDER_BLOCK];
    let mut right = [0_f32; RENDER_BLOCK];
    let mut rendered = 0;
    while rendered < total {
        let count = RENDER_BLOCK.min(total - rendered);
        sequencer.render(&mut left[..count], &mut right[..count]);
        if let Some((sink_left, sink_right)) = sink.as_mut() {
            sink_left.extend_from_slice(&left[..count]);
            sink_right.extend_from_slice(&right[..count]);
        }
        rendered += count;
    }
}

fn bench_midi(
    sound_font: &Arc<SoundFont>,
    reps: usize,
    dump: Option<&Path>,
) -> Result<(), BenchError> {
    let bytes = build_midi(64);
    let mut reader = bytes.as_slice();
    let midi = Arc::new(MidiFile::new(&mut reader)?);
    // Render a full second past the end of the file so that the tail of the
    // release is included.
    let total = (SAMPLE_RATE as f64 * (midi.get_length() + 1.0)) as usize;

    if let Some(dir) = dump {
        let mut left = Vec::with_capacity(total);
        let mut right = Vec::with_capacity(total);
        render_sequence(sound_font, &midi, total, Some((&mut left, &mut right)));
        write_dump(dir, "midi-chord.f32", &left, &right)?;
    }

    let timing = measure(reps, MIN_SAMPLE_SECONDS, || {
        render_sequence(sound_font, &midi, total, None);
    });
    report("midi/chord", timing, reps);
    Ok(())
}

/// Writes interleaved little-endian f32 pairs, for a bit-exact comparison of
/// two builds with `cmp`.
fn write_dump(dir: &Path, name: &str, left: &[f32], right: &[f32]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut bytes = Vec::with_capacity(left.len() * 8);
    for (&l, &r) in left.iter().zip(right) {
        bytes.extend_from_slice(&l.to_le_bytes());
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    let path = dir.join(name);
    std::fs::write(&path, &bytes)?;
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

fn bench_synth(sound_font: &Arc<SoundFont>, reps: usize) {
    // A dense chord: sixteen notes on each of four channels.
    const SECONDS: usize = 30;
    for &(label, effects, block_size) in &[
        ("synth/effects-block64", true, 64_usize),
        ("synth/dry-block64", false, 64),
        ("synth/effects-block256", true, 256),
    ] {
        let total = SAMPLE_RATE as usize * SECONDS;
        // Allocated once: zeroing 1.3M samples is not what is being measured.
        let mut left = vec![0_f32; total];
        let mut right = vec![0_f32; total];
        let timing = measure(reps, MIN_SAMPLE_SECONDS, || {
            let mut settings = SynthesizerSettings::new(SAMPLE_RATE);
            settings.block_size = block_size;
            settings.enable_reverb_and_chorus = effects;
            settings.maximum_polyphony = 64;
            let mut synthesizer = Synthesizer::new(sound_font, &settings).unwrap();
            for channel in 0..4 {
                for k in 0..16 {
                    synthesizer.note_on(channel, 36 + k * 3, 100);
                }
            }
            synthesizer.render(&mut left, &mut right);
        });
        report(label, timing, reps);
    }
}

fn parse_options(args: &[String]) -> Result<(usize, Option<PathBuf>, Option<PathBuf>), String> {
    let mut reps = 3;
    let mut font = None;
    let mut dump = None;
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        let mut value = || {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{argument} needs a value"))
        };
        match argument {
            "--reps" => {
                reps = value()?
                    .parse()
                    .map_err(|_| "--reps needs a number".to_string())?
            }
            "--font" => font = Some(PathBuf::from(value()?)),
            "--dump" => dump = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }
    if reps == 0 {
        return Err("--reps must be at least 1".to_string());
    }
    Ok((reps, font, dump))
}

pub fn run(args: &[String]) -> i32 {
    let (reps, font, dump) = match parse_options(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("bench: {message}");
            return 2;
        }
    };

    if cfg!(debug_assertions) {
        eprintln!("note: this is a debug build, so the numbers are meaningless.");
        eprintln!("      run `cargo xtask-bench` instead.");
    }

    let samples = repo_root().join("samples");

    for name in FONT_NAMES {
        bench_load(&samples, name, reps);
    }

    // The synthesis benchmarks need a font; take the smallest one available so
    // that the load does not dominate the run.
    let explicit_font = font.is_some();
    let font_path = match font {
        Some(path) => path,
        None => ["TimGM6mb.sf2", "FluidR3Mono_GM.sf2"]
            .iter()
            .map(|name| samples.join(name))
            .find(|path| path.is_file())
            .unwrap_or_else(|| samples.join("TimGM6mb.sf2")),
    };
    if !font_path.is_file() {
        if explicit_font {
            eprintln!("bench: {} does not exist", font_path.display());
            return 1;
        }
        println!(
            "skip: {} is missing, run `cargo xtask fetch-fonts`",
            font_path.display()
        );
        return 0;
    }

    let mut file = match File::open(&font_path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("bench: cannot open {}: {err}", font_path.display());
            return 1;
        }
    };
    let sound_font = match SoundFont::new(&mut file) {
        Ok(sound_font) => Arc::new(sound_font),
        Err(err) => {
            eprintln!("bench: cannot load {}: {err}", font_path.display());
            return 1;
        }
    };
    drop(file);

    if let Err(err) = bench_midi(&sound_font, reps, dump.as_deref()) {
        eprintln!("bench: {err}");
        return 1;
    }
    bench_synth(&sound_font, reps);
    0
}
