//! Development tasks for this workspace, invoked as `cargo xtask <task>`.
//!
//! * `fetch-fonts`       - downloads the test fonts into samples/.
//! * `generate-goldens`  - regenerates the golden test data in rustysynth_test/.
//! * `bench`             - benchmarks the synthesizer and the SoundFont loader.

mod bench;
mod fetch;
mod generate;

fn usage() {
    eprintln!(
        "Usage: cargo xtask <task>\n\n\
         Tasks:\n  \
         fetch-fonts       download the test fonts into samples/\n  \
         generate-goldens  regenerate the golden test data in rustysynth_test/\n  \
         bench             benchmark rendering and SoundFont loading\n\n\
         The bench task takes --reps <n>, --font <path> and --dump <dir>."
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();
    let code = match task.as_deref() {
        Some("fetch-fonts") => fetch::run(),
        Some("generate-goldens") => generate::run(),
        Some("bench") => bench::run(&rest),
        Some(other) => {
            eprintln!("unknown task: {other}\n");
            usage();
            2
        }
        None => {
            usage();
            1
        }
    };
    std::process::exit(code);
}
