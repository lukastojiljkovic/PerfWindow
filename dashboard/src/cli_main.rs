//! The `perfwindow-cli` console program. It is built without the
//! `windows_subsystem` attribute `main.rs` uses, so it keeps a console and its
//! output reaches the terminal it was started from.

fn main() -> std::process::ExitCode {
    perfwindow::cli::run()
}
