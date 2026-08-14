//! The three MVP log formats (TDR §8). Syslog / Apache / Docker formats are P1 and
//! not represented here (see proposal.md Non-Goals).

/// A log format the parser can produce entries from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// One JSON object per line.
    Json,
    /// `key=value` pairs separated by whitespace, quoted values allowed.
    Logfmt,
    /// Unstructured text, one entry per non-empty line. Never fails to "parse".
    Plain,
}

impl Format {
    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Logfmt => "logfmt",
            Format::Plain => "plain",
        }
    }
}
