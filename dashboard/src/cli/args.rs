//! Command-line arguments of `perfwindow-cli`. Parsing is pure so every flag
//! and every error can be tested without starting the sensor service.

/// How a report is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

/// What one run of the command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub format: Format,
    /// Seconds between reports with `--watch`; `None` prints one report.
    pub watch: Option<u32>,
}

/// The parsed command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Run(Options),
    Help,
    Version,
}

/// Seconds `--watch` uses when it is given without a value.
const DEFAULT_WATCH_S: u32 = 2;
/// Shortest and longest `--watch` interval. The sensor service's own refresh
/// interval is set from it, so anything outside this range is refused rather
/// than clamped.
const WATCH_RANGE_S: std::ops::RangeInclusive<u32> = 1..=60;

/// The help text: usage, every flag, the exit codes and where the command is
/// switched on.
pub const USAGE: &str = "\
Usage: perfwindow-cli [--json] [--watch [SECONDS]]
       perfwindow-cli --help | --version

Prints every sensor reading the PerfWindow window shows, then exits.

Options:
  --json             JSON instead of text, for scripts and AI agents
  --watch [SECONDS]  print a new report every SECONDS (1 to 60, default 2)
                     until stopped with Ctrl+C
  -h, --help         show this help
  -V, --version      show the version

Exit codes:
  0  report printed
  2  bad arguments
  3  command-line readings are off
  4  the sensor service is unavailable

The command must be turned on in PerfWindow -> Settings -> Command line.
";

/// Parse the arguments after `argv[0]`.
///
/// `--watch` takes an optional value: the next argument is the interval only
/// when it does not start with `-`, so `--watch --json` works and
/// `--watch x` is an error naming `x`.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut format = Format::Text;
    let mut watch = None;
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = Format::Json,
            "--watch" => {
                let seconds = match args.next_if(|next| !next.starts_with('-')) {
                    Some(value) => parse_seconds(&value)?,
                    None => DEFAULT_WATCH_S,
                };
                watch = Some(seconds);
            }
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            other => return Err(format!("unknown argument \"{other}\"")),
        }
    }
    Ok(Command::Run(Options { format, watch }))
}

fn parse_seconds(value: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(seconds) if WATCH_RANGE_S.contains(&seconds) => Ok(seconds),
        _ => Err(format!(
            "--watch needs a whole number of seconds from 1 to 60, not \"{value}\""
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn run(list: &[&str]) -> Options {
        match parse(args(list)) {
            Ok(Command::Run(options)) => options,
            other => panic!("expected a run, got {other:?}"),
        }
    }

    #[test]
    fn no_arguments_prints_one_text_report() {
        assert_eq!(
            run(&[]),
            Options {
                format: Format::Text,
                watch: None
            }
        );
    }

    #[test]
    fn json_switches_the_format() {
        assert_eq!(run(&["--json"]).format, Format::Json);
    }

    #[test]
    fn watch_without_a_value_uses_two_seconds() {
        assert_eq!(run(&["--watch"]).watch, Some(2));
    }

    #[test]
    fn watch_takes_the_next_argument_as_its_value() {
        assert_eq!(run(&["--watch", "5"]).watch, Some(5));
        assert_eq!(run(&["--watch", "1"]).watch, Some(1));
        assert_eq!(run(&["--watch", "60"]).watch, Some(60));
    }

    #[test]
    fn watch_does_not_swallow_the_next_flag() {
        let options = run(&["--watch", "--json"]);
        assert_eq!(options.watch, Some(2));
        assert_eq!(options.format, Format::Json);
    }

    #[test]
    fn watch_values_outside_one_to_sixty_are_errors() {
        assert!(parse(args(&["--watch", "0"])).is_err());
        assert!(parse(args(&["--watch", "61"])).is_err());
    }

    #[test]
    fn a_non_numeric_watch_value_is_an_error_naming_it() {
        let error = parse(args(&["--watch", "x"])).unwrap_err();
        assert!(error.contains("\"x\""), "{error}");
    }

    #[test]
    fn a_negative_watch_value_is_an_unknown_argument() {
        let error = parse(args(&["--watch", "-5"])).unwrap_err();
        assert!(error.contains("\"-5\""), "{error}");
    }

    #[test]
    fn help_and_version_in_both_spellings() {
        assert_eq!(parse(args(&["-h"])), Ok(Command::Help));
        assert_eq!(parse(args(&["--help"])), Ok(Command::Help));
        assert_eq!(parse(args(&["-V"])), Ok(Command::Version));
        assert_eq!(parse(args(&["--version"])), Ok(Command::Version));
    }

    #[test]
    fn an_unknown_argument_is_an_error_naming_it() {
        let error = parse(args(&["--frobnicate"])).unwrap_err();
        assert!(error.contains("--frobnicate"), "{error}");
    }

    #[test]
    fn usage_documents_every_flag_and_exit_code() {
        for needle in [
            "--json",
            "--watch",
            "--help",
            "--version",
            "2  bad",
            "3  ",
            "4  ",
        ] {
            assert!(USAGE.contains(needle), "USAGE is missing {needle}");
        }
        assert!(USAGE.contains("PerfWindow -> Settings -> Command line"));
    }
}
