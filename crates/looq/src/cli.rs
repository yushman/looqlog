use std::io::IsTerminal;
use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;

/// looq — a single-binary CLI that opens a local web UI for a log file or stdin.
///
/// File mode: `looq app.log` prints `app.log` as a hint of which file to open in the
/// browser; the file is picked and parsed entirely client-side (ADR-0002, ADR-0007).
/// Stdin mode: `myapp | looq` streams stdin lines to connected browsers over `/ws`.
#[derive(Parser, Debug)]
#[command(name = "looq", version, about, disable_help_subcommand = true)]
pub struct Cli {
    /// Log file to open in the browser. This process never opens or reads it in file
    /// mode (ADR-0002, ADR-0007) — it is shown in the page as a hint only.
    pub path: Option<PathBuf>,

    /// Port to bind (0 = ask the OS for a free port).
    #[arg(long, default_value_t = 7891)]
    pub port: u16,

    /// Bind address. Anything other than 127.0.0.1 prints a mandatory exposure
    /// warning (ADR-0003): live stdin logs would become reachable from the network
    /// without authentication.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Auto-open the default browser once the server is ready.
    #[arg(long)]
    pub open: bool,

    /// Suppress auto-open even if `--open` was passed.
    #[arg(long)]
    pub no_browser: bool,

    /// Force stdin mode even if stdin looks like a TTY.
    #[arg(long)]
    pub stdin: bool,

    /// Max lines kept for live tail. Stdin mode only in this change — the ring
    /// buffer that will honor it lands in the `live-tail` change; here it is parsed
    /// and reported, and if given in file mode a note is printed since it has no
    /// effect there.
    #[arg(long)]
    pub max_lines: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    File,
    Stdin,
}

impl Cli {
    /// stdin mode when `--stdin` was passed or stdin is not a TTY (e.g. it's a pipe);
    /// file mode otherwise (TDR §6/§7, `cli` spec "Mode selection"). The real TTY
    /// check happens once here; the decision itself lives in [`mode_for`], which is
    /// unit-testable without a real terminal (see `tests::mode_for` below).
    pub fn mode(&self) -> Mode {
        mode_for(self.stdin, std::io::stdin().is_terminal())
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

/// Pure mode-selection decision, factored out of [`Cli::mode`] so it can be unit
/// tested without needing a real TTY (`cli` spec, "Mode selection between file and
/// stdin").
pub fn mode_for(stdin_flag_passed: bool, stdin_is_terminal: bool) -> Mode {
    if stdin_flag_passed || !stdin_is_terminal {
        Mode::Stdin
    } else {
        Mode::File
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdin_flag_forces_stdin_mode_even_on_a_tty() {
        assert_eq!(mode_for(true, true), Mode::Stdin);
    }

    #[test]
    fn non_tty_stdin_selects_stdin_mode_without_the_flag() {
        assert_eq!(mode_for(false, false), Mode::Stdin);
    }

    #[test]
    fn tty_stdin_with_no_flag_selects_file_mode() {
        assert_eq!(mode_for(false, true), Mode::File);
    }
}
