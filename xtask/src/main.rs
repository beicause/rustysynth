//! Development tasks for this workspace, invoked as `cargo xtask <task>`.
//!
//! * `fetch-fonts`       - downloads the test fonts into samples/.
//! * `generate-goldens`  - regenerates the golden test data in rustysynth_test/.

mod fetch;
mod generate;

fn usage() {
    eprintln!(
        "Usage: cargo xtask <task>\n\n\
         Tasks:\n  \
         fetch-fonts       download the test fonts into samples/\n  \
         generate-goldens  regenerate the golden test data in rustysynth_test/"
    );
}

fn main() {
    let task = std::env::args().nth(1);
    let code = match task.as_deref() {
        Some("fetch-fonts") => fetch::run(),
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
