use std::io::IsTerminal;
use std::net::IpAddr;
use std::path::PathBuf;

use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser};

/// looqlog — a single-binary CLI that opens a local web UI for log files and stdin.
///
/// `looqlog a.log b.log` reads and parses each file as its own source; `myapp | looqlog`
/// parses stdin as it arrives. Both can be combined, and more files can be uploaded from
/// the page (ADR-0010, ADR-0011). Each source has its own URL; `/` lists them.
#[derive(Parser, Debug)]
#[command(name = "looqlog", version, about, disable_help_subcommand = true)]
pub struct Cli {
    /// Log files to open, each as its own source. Read once at startup by this process,
    /// not followed (ADR-0010).
    pub paths: Vec<PathBuf>,

    /// Port to bind (0 = ask the OS for a free port). When not given and 7891 is busy,
    /// the next free port up to 7900 is used.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Bind address. Anything other than 127.0.0.1 prints a mandatory exposure
    /// warning (ADR-0003): log contents — files and stdin — would become reachable
    /// from the network.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Open the default browser once the server is ready: one tab per source, or the
    /// source list when there are none.
    #[arg(long)]
    pub open: bool,

    /// Suppress auto-open even if `--open` was passed.
    #[arg(long)]
    pub no_browser: bool,

    /// Read stdin even if it looks like a TTY.
    #[arg(long)]
    pub stdin: bool,

    /// Max lines kept for the stdin source (the oldest are evicted). Given without a
    /// stdin source it has no effect and a note says so.
    #[arg(long)]
    pub max_lines: Option<usize>,
}

pub const DEFAULT_PORT: u16 = 7891;

/// Ports tried in order when `--port` was not given (`local-server` spec).
pub const FALLBACK_PORTS: std::ops::RangeInclusive<u16> = DEFAULT_PORT..=7900;

impl Cli {
    /// Parses the arguments, and whether `--port` was given explicitly.
    pub fn parse_with_port_source() -> (Self, bool) {
        let matches = Self::command().get_matches();
        let explicit = matches.value_source("port") == Some(ValueSource::CommandLine);
        let cli = Self::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
        (cli, explicit)
    }

    /// A stdin source is added when `--stdin` was passed or stdin is not a TTY (e.g.
    /// it's a pipe), independent of any paths (`cli` spec). The real TTY check happens
    /// once here; the decision itself lives in [`reads_stdin`].
    pub fn reads_stdin(&self) -> bool {
        reads_stdin(self.stdin, std::io::stdin().is_terminal())
    }

    pub fn resolved_host(&self) -> Result<IpAddr, String> {
        self.host
            .parse::<IpAddr>()
            .map_err(|_| format!("invalid --host value: {:?}", self.host))
    }

    pub const DEFAULT_MAX_LINES: usize = 100_000;

    pub fn max_lines_or_default(&self) -> usize {
        self.max_lines.unwrap_or(Self::DEFAULT_MAX_LINES)
    }
}

/// Pure stdin decision, factored out of [`Cli::reads_stdin`] so it can be unit tested
/// without a real TTY.
pub fn reads_stdin(stdin_flag_passed: bool, stdin_is_terminal: bool) -> bool {
    stdin_flag_passed || !stdin_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdin_flag_reads_stdin_even_on_a_tty() {
        assert!(reads_stdin(true, true));
    }

    #[test]
    fn non_tty_stdin_is_read_without_the_flag() {
        assert!(reads_stdin(false, false));
    }

    #[test]
    fn tty_stdin_with_no_flag_is_not_read() {
        assert!(!reads_stdin(false, true));
    }

    #[test]
    fn several_paths_are_accepted() {
        let cli = Cli::try_parse_from(["looqlog", "a.log", "b.log"]).unwrap();
        assert_eq!(cli.paths, [PathBuf::from("a.log"), PathBuf::from("b.log")]);
    }
}
