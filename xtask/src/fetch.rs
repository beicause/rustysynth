//! `cargo xtask fetch-sf2` - downloads/extracts the SoundFont2 test fonts
//! into `samples/`:
//!
//! * `FluidR3Mono_GM.sf2` - extracted from the bundled
//!   `samples/FluidR3Mono_GM.tar.zst` (decompressed in-process with the
//!   `zstd` crate, archive unpacked with the `tar` crate).
//! * `TimGM6mb.sf2`       - downloaded (official host, then verified mirrors).
//!
//! The task is idempotent: files that already exist are left untouched.
//! Every `.sf2` under `samples/` is ignored by git (see the root
//! `.gitignore`).

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/xtask
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn download(url: &str, target: &Path) -> Result<(), String> {
    let agent = ureq::config::Config::builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_recv_body(Some(Duration::from_secs(120)))
        .build()
        .new_agent();
    let mut response = agent.get(url).call().map_err(|err| err.to_string())?;
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|err| err.to_string())?;
    fs::write(target, body).map_err(|err| err.to_string())
}

fn fetch_fluid(samples_dir: &Path) -> Result<(), String> {
    let target = samples_dir.join("FluidR3Mono_GM.sf2");
    let archive = samples_dir.join("FluidR3Mono_GM.tar.zst");

    if target.is_file() {
        println!("exists: {}", target.display());
        return Ok(());
    }
    if !archive.is_file() {
        println!(
            "skip: '{}' not found, cannot extract FluidR3Mono_GM.sf2",
            archive.display()
        );
        return Ok(());
    }

    println!("extracting {} ...", target.display());
    let file = File::open(&archive).map_err(|err| err.to_string())?;
    let decoder = zstd::stream::read::Decoder::new(file).map_err(|err| err.to_string())?;
    let mut archive_reader = tar::Archive::new(decoder);
    archive_reader
        .unpack(samples_dir)
        .map_err(|err| err.to_string())?;

    if target.is_file() {
        println!("saved: {}", target.display());
        Ok(())
    } else {
        Err(format!("the archive did not contain {}", target.display()))
    }
}

/// Mirrors for TimGM6mb.sf2. The mirror copies were verified to produce the
/// same sample data that the timgm6mb_* golden tests expect.
const TIMGM6MB_MIRRORS: &[&str] = &[
    "https://member.keymusician.com/Member/TimGM6mb.sf2",
    "https://raw.githubusercontent.com/arbruijn/TimGM6mb/master/TimGM6mb.sf2",
    "https://raw.githubusercontent.com/deepin-community/timgm6mb-soundfont/master/TimGM6mb.sf2",
];

fn fetch_timgm6mb(samples_dir: &Path) -> Result<(), String> {
    let target = samples_dir.join("TimGM6mb.sf2");

    if target.is_file() {
        println!("exists: {}", target.display());
        return Ok(());
    }

    for url in TIMGM6MB_MIRRORS {
        println!("downloading {url} ...");
        match download(url, &target) {
            Ok(()) => {
                println!("saved: {}", target.display());
                return Ok(());
            }
            Err(err) => {
                let _ = fs::remove_file(&target);
                eprintln!("  failed: {err}");
            }
        }
    }

    Err("could not download TimGM6mb.sf2 from any source".to_string())
}

pub fn run() -> i32 {
    let samples_dir = repo_root().join("samples");
    if let Err(err) = fs::create_dir_all(&samples_dir) {
        eprintln!("error: {err}");
        return 1;
    }

    let mut failed = false;

    if let Err(err) = fetch_fluid(&samples_dir) {
        eprintln!("error: {err}");
        failed = true;
    }
    if let Err(err) = fetch_timgm6mb(&samples_dir) {
        eprintln!("error: {err}");
        failed = true;
    }

    if failed {
        1
    } else {
        println!("done: {}", samples_dir.display());
        0
    }
}
