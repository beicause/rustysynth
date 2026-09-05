//! Development tasks for this workspace, invoked as `cargo xtask <task>`.
//!
//! * `fetch-sf2`        - downloads/extracts the SF2 test fonts into samples/.
//! * `generate-goldens` - regenerates the golden test data in rustysynth_test/.

mod fetch;
mod generate;

fn usage() {
    eprintln!(
        "Usage: cargo xtask <task>\n\n\
         Tasks:\n  \
         fetch-sf2         download or extract the SF2 test fonts into samples/\n  \
         generate-goldens  regenerate the golden test data in rustysynth_test/"
    );
}

fn main() {
    let task = std::env::args().nth(1);
    let code = match task.as_deref() {
        Some("fetch-sf2") => fetch::run(),
        Some("generate-goldens") => generate::run(),
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