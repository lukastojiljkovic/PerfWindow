//! The terminal around `perfwindow-cli`: what stdout can take, the glyphs
//! its font can draw, escape codes on the Windows console, and the status
//! line on stderr while the sensors load.

use super::report::Style;
use ratatui::symbols::bar;
use std::ffi::OsStr;
use std::io::{IsTerminal, Stderr, Write};

/// The report style stdout takes: ASCII unless it is a terminal, coloured
/// when that terminal takes escape codes and colour is allowed.
pub fn report_style(no_color: bool) -> Style {
    let terminal = std::io::stdout().is_terminal();
    style_for(
        terminal,
        terminal && color_allowed(no_color) && enable_escapes(),
    )
}

/// The pure decision behind [`report_style`].
fn style_for(terminal: bool, color: bool) -> Style {
    match (terminal, color) {
        (false, _) => Style::Ascii,
        (true, true) => Style::Color,
        (true, false) => Style::Plain,
    }
}

/// Whether colour is allowed: no `--no-color`, `NO_COLOR` unset or empty,
/// and `TERM` not `dumb` (<https://no-color.org>, clig.dev).
pub fn color_allowed(no_color: bool) -> bool {
    !no_color
        && env_allows_color(
            std::env::var_os("NO_COLOR").as_deref(),
            std::env::var_os("TERM").as_deref(),
        )
}

/// The pure decision behind [`color_allowed`], on the two variables.
fn env_allows_color(no_color: Option<&OsStr>, term: Option<&OsStr>) -> bool {
    no_color.is_none_or(OsStr::is_empty) && term != Some(OsStr::new("dumb"))
}

/// Whether stdin and stdout are both on a terminal that can hold a
/// full-screen view, so a plain run may open the live dashboard.
pub fn interactive() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::env::var_os("TERM").as_deref() != Some(OsStr::new("dumb"))
}

/// Whether the terminal draws 24-bit colour: `COLORTERM` says so, or it is
/// Windows Terminal, which sets `WT_SESSION`.
pub fn truecolor() -> bool {
    supports_truecolor(
        std::env::var_os("COLORTERM").as_deref(),
        std::env::var_os("WT_SESSION").is_some(),
    )
}

/// The pure decision behind [`truecolor`].
fn supports_truecolor(colorterm: Option<&OsStr>, windows_terminal: bool) -> bool {
    windows_terminal || colorterm.is_some_and(|value| value == "truecolor" || value == "24bit")
}

/// The block glyphs the live dashboard draws with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyphs {
    /// The steps a sparkline's bars rise in.
    pub bars: bar::Set<'static>,
    /// The mark before the wordmark in the footer.
    pub mark: &'static str,
}

impl Glyphs {
    /// Every block element: the eighth blocks and the window's `▚` mark.
    pub const FULL: Glyphs = Glyphs {
        bars: bar::NINE_LEVELS,
        mark: "▚",
    };
    /// Only code page 437's block elements: classic conhost's default font
    /// has no others, and draws them as missing-glyph boxes.
    pub const CP437: Glyphs = Glyphs {
        bars: bar::THREE_LEVELS,
        mark: "▌",
    };
}

/// The glyphs the terminal's font can draw: all of them in Windows Terminal,
/// which sets `WT_SESSION`, and otherwise only those classic conhost can.
pub fn glyphs() -> Glyphs {
    glyphs_for(std::env::var_os("WT_SESSION").is_some())
}

/// The pure decision behind [`glyphs`].
fn glyphs_for(windows_terminal: bool) -> Glyphs {
    if windows_terminal {
        Glyphs::FULL
    } else {
        Glyphs::CP437
    }
}

/// Turn on escape-code processing for stdout's console. `false` when stdout
/// is no console or the console refuses, so the report goes out uncoloured.
#[cfg(windows)]
fn enable_escapes() -> bool {
    use windows::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_OUTPUT_HANDLE,
    };
    // SAFETY: the handle is the process's own stdout, and the mode is read
    // into a local before the same handle is given it back with one more flag.
    unsafe {
        let Ok(stdout) = GetStdHandle(STD_OUTPUT_HANDLE) else {
            return false;
        };
        let mut mode = CONSOLE_MODE::default();
        GetConsoleMode(stdout, &mut mode).is_ok()
            && (mode.contains(ENABLE_VIRTUAL_TERMINAL_PROCESSING)
                || SetConsoleMode(stdout, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING).is_ok())
    }
}

#[cfg(not(windows))]
fn enable_escapes() -> bool {
    true
}

/// One line on stderr that says how far the sensors have loaded. It is drawn
/// only while stderr is a terminal and rewritten in place with a carriage
/// return, so it needs no escape codes; piped stderr gets only the notices.
/// A write that fails is dropped: the line is only ever a courtesy.
pub struct Status<W: Write = Stderr> {
    out: W,
    enabled: bool,
    shown: String,
}

impl Status {
    /// A status line, drawn only if stderr is a terminal.
    pub fn new() -> Status {
        let out = std::io::stderr();
        Status {
            enabled: out.is_terminal(),
            out,
            shown: String::new(),
        }
    }
}

impl<W: Write> Status<W> {
    /// Show `text` in place of the current status.
    pub fn show(&mut self, text: &str) {
        if self.enabled && text != self.shown {
            let _ = write!(self.out, "{}", overwrite(&self.shown, text));
            self.shown = text.to_string();
        }
    }

    /// Show `text` even where no status line is drawn: on a terminal in its
    /// place, so the next status or [`Status::finish`] erases it rather than
    /// leaving it in the scrollback, and on piped stderr as a line of its own.
    pub fn notice(&mut self, text: &str) {
        if self.enabled {
            self.show(text);
        } else {
            let _ = writeln!(self.out, "{text}");
        }
    }

    /// Erase the line, so a message or the report starts on a clean one.
    pub fn clear(&mut self) {
        if !self.shown.is_empty() {
            let _ = write!(self.out, "{}\r", overwrite(&self.shown, ""));
            self.shown.clear();
        }
    }

    /// Erase the line for good: once the first report or the dashboard is
    /// up, the status does not come back.
    pub fn finish(&mut self) {
        self.clear();
        self.enabled = false;
    }
}

/// What turns a terminal line showing `old` into one showing `new`: a
/// carriage return, `new`, and spaces over whatever is left of `old`.
fn overwrite(old: &str, new: &str) -> String {
    let pad = old.chars().count().saturating_sub(new.chars().count());
    format!("\r{new}{:pad$}", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdout_that_is_not_a_terminal_gets_ascii() {
        assert_eq!(style_for(false, false), Style::Ascii);
        assert_eq!(style_for(true, false), Style::Plain);
        assert_eq!(style_for(true, true), Style::Color);
    }

    #[test]
    fn no_color_set_and_non_empty_or_a_dumb_term_turn_colour_off() {
        let os = |s: &'static str| Some(OsStr::new(s));
        assert!(env_allows_color(None, None));
        assert!(env_allows_color(os(""), os("xterm-256color")));
        assert!(!env_allows_color(os("1"), None));
        assert!(!env_allows_color(os("0"), None));
        assert!(!env_allows_color(None, os("dumb")));
    }

    #[test]
    fn truecolor_comes_from_colorterm_or_windows_terminal() {
        assert!(supports_truecolor(Some(OsStr::new("truecolor")), false));
        assert!(supports_truecolor(Some(OsStr::new("24bit")), false));
        assert!(supports_truecolor(None, true));
        assert!(!supports_truecolor(Some(OsStr::new("yes")), false));
        assert!(!supports_truecolor(None, false));
    }

    #[test]
    fn only_windows_terminal_gets_glyphs_outside_code_page_437() {
        assert_eq!(glyphs_for(true), Glyphs::FULL);
        assert_eq!(glyphs_for(false), Glyphs::CP437);
        // Classic conhost's default font draws these, and no other block element.
        let drawable = [" ", "▀", "▄", "█", "▌", "▐", "░", "▒", "▓"];
        let Glyphs { bars, mark } = Glyphs::CP437;
        let steps = [
            bars.full,
            bars.seven_eighths,
            bars.three_quarters,
            bars.five_eighths,
            bars.half,
            bars.three_eighths,
            bars.one_quarter,
            bars.one_eighth,
            bars.empty,
            mark,
        ];
        assert!(steps.iter().all(|s| drawable.contains(s)), "{steps:?}");
    }

    fn status(terminal: bool) -> Status<Vec<u8>> {
        Status {
            out: Vec::new(),
            enabled: terminal,
            shown: String::new(),
        }
    }

    #[test]
    fn a_notice_on_a_terminal_is_erased_once_the_status_finishes() {
        let mut status = status(true);
        status.notice("Approve the prompt.");
        status.finish();
        assert_eq!(
            String::from_utf8(status.out).unwrap(),
            "\rApprove the prompt.\r                   \r"
        );
    }

    #[test]
    fn a_notice_on_piped_stderr_is_a_line_of_its_own() {
        let mut status = status(false);
        status.notice("Approve the prompt.");
        status.show("Loading sensors…");
        status.finish();
        assert_eq!(
            String::from_utf8(status.out).unwrap(),
            "Approve the prompt.\n"
        );
    }

    #[test]
    fn a_shorter_status_blanks_out_the_rest_of_the_longer_one() {
        assert_eq!(overwrite("", "Loading…"), "\rLoading…");
        assert_eq!(
            overwrite("Loading sensors: gpu", "Done"),
            "\rDone                "
        );
        assert_eq!(overwrite("abc", ""), "\r   ");
    }
}
