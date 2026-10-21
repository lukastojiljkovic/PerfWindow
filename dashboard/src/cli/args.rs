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
    /// `--once`: one text report even in a terminal, where a plain run opens
    /// the live dashboard.
    pub once: bool,
    /// `--no-color`: no colour in the report or the dashboard.
    pub no_color: bool,
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

/// The help text: what the command does, usage, every flag, examples, the
/// exit codes and where the command is switched on. ASCII only, so it reads
/// the same piped into any code page.
pub const USAGE: &str = "\
perfwindow-cli: every sensor reading the PerfWindow window shows.

Usage: perfwindow-cli [--once | --watch [SECONDS]] [--json] [--no-color]
       perfwindow-cli --help | --version

In a terminal it opens a live dashboard that redraws on every new sample;
q, Esc or Ctrl+C leaves it. When stdin or stdout is not a terminal (piped,
redirected, or run by a script or an AI agent) it prints one text report
instead: plain ASCII, one \"Label: value\" reading per line.

Options:
  --once             print one text report and exit, even in a terminal
  --json             print the readings as JSON (temperatures in Celsius)
  --watch [SECONDS]  print a new report every SECONDS (1 to 60, default 2)
                     until stopped with Ctrl+C; JSON is one object per line
  --no-color         no colour (also with NO_COLOR set, or TERM=dumb)
  -h, --help         show this help
  -V, --version      show the version

Examples:
  perfwindow-cli                   the live dashboard
  perfwindow-cli --once            one text report
  perfwindow-cli --json            one JSON report, for scripts and AI agents
  perfwindow-cli --watch 5 --json  a JSON object every 5 seconds (NDJSON)

Exit codes:
  0  success
  2  bad arguments
  3  command-line readings are off
  4  the sensor service is unavailable or stopped

Turn the command on in PerfWindow -> Settings -> Command line. It starts the
sensor service as the window does, so Windows asks for approval, and it
cannot read the sensors while the window is open.
";

/// Parse the arguments after `argv[0]`.
///
/// `--watch` takes an optional value: the next argument is the interval only
/// when it does not start with `-`, so `--watch --json` works and
/// `--watch x` is an error naming `x`.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut format = Format::Text;
    let mut watch = None;
    let mut once = false;
    let mut no_color = false;
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = Format::Json,
            "--once" => once = true,
            "--no-color" => no_color = true,
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
    if once && watch.is_some() {
        return Err("--once and --watch cannot be used together".to_string());
    }
    Ok(Command::Run(Options {
        format,
        watch,
        once,
        no_color,
    }))
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
    fn no_arguments_is_a_plain_text_run() {
        assert_eq!(
            run(&[]),
            Options {
                format: Format::Text,
                watch: None,
                once: false,
                no_color: false,
            }
        );
    }

    #[test]
    fn json_switches_the_format() {
        assert_eq!(run(&["--json"]).format, Format::Json);
    }

    #[test]
    fn once_and_no_color_are_switches() {
        let options = run(&["--once", "--no-color"]);
        assert!(options.once && options.no_color);
        assert_eq!(options.watch, None);
    }

    #[test]
    fn once_and_watch_contradict_each_other() {
        let error = parse(args(&["--once", "--watch", "5"])).unwrap_err();
        assert!(
            error.contains("--once") && error.contains("--watch"),
            "{error}"
        );
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
            "--once",
            "--json",
            "--watch",
            "--no-color",
            "--help",
            "--version",
            "0  success",
            "2  bad",
            "3  ",
            "4  ",
            "perfwindow-cli --json ",
        ] {
            assert!(USAGE.contains(needle), "USAGE is missing {needle}");
        }
        assert!(USAGE.contains("PerfWindow -> Settings -> Command line"));
        assert!(USAGE.is_ascii());
        assert!(USAGE.lines().all(|line| line.len() <= 80));
    }
}
