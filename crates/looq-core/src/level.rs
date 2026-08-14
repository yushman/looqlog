//! Level extraction and normalisation (design.md D6). A fixed alias table, no
//! invented default: a line with no recognisable level stays levelless rather than
//! being guessed at INFO.

/// A normalised log level. Ordered from least to most severe, matching TDR §9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Fatal => "FATAL",
        }
    }

    fn from_canonical(s: &str) -> Option<Level> {
        match s {
            "TRACE" => Some(Level::Trace),
            "DEBUG" => Some(Level::Debug),
            "INFO" => Some(Level::Info),
            "WARN" => Some(Level::Warn),
            "ERROR" => Some(Level::Error),
            "FATAL" => Some(Level::Fatal),
            _ => None,
        }
    }
}

/// Fixed alias table (design.md D6). Syslog numeric severities are P1, not here.
const ALIASES: &[(&str, &str)] = &[("WARNING", "WARN"), ("ERR", "ERROR"), ("CRITICAL", "FATAL")];

/// Normalise a raw level string (from a dedicated field or a message scan match)
/// against the alias table and the canonical set. Returns `None` for anything that
/// does not resolve to a known level — callers must not substitute a default.
pub fn normalize(raw: &str) -> Option<Level> {
    let upper = raw.trim().to_ascii_uppercase();
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| *alias == upper)
        .map(|(_, canonical)| *canonical)
        .unwrap_or(upper.as_str());
    Level::from_canonical(canonical)
}

/// Scan free text for the first case-insensitive match of a known level word or
/// alias, used by the plain-text parser and as the JSON/logfmt fallback when no
/// dedicated level field is present.
pub fn scan_message(text: &str) -> Option<Level> {
    // Word-boundary scan without pulling in `regex` for a fixed, small vocabulary:
    // split on non-alphabetic runs and test each token against the alias/canonical
    // tables. Cheaper than a regex crate dependency for this one job, and this
    // module has no other reason to depend on `regex`.
    for token in text.split(|c: char| !c.is_ascii_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        if let Some(level) = normalize(token) {
            return Some(level);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_known_aliases() {
        assert_eq!(normalize("warning"), Some(Level::Warn));
        assert_eq!(normalize("ERR"), Some(Level::Error));
        assert_eq!(normalize("Critical"), Some(Level::Fatal));
    }

    #[test]
    fn canonical_values_pass_through() {
        assert_eq!(normalize("info"), Some(Level::Info));
        assert_eq!(normalize("ERROR"), Some(Level::Error));
    }

    #[test]
    fn unknown_value_is_absent() {
        assert_eq!(normalize("banana"), None);
    }

    #[test]
    fn scan_finds_first_match() {
        assert_eq!(
            scan_message("this is a WARN about something"),
            Some(Level::Warn)
        );
    }

    #[test]
    fn scan_finds_nothing_when_absent() {
        assert_eq!(scan_message("nothing to see here"), None);
    }
}
