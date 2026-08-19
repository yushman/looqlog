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

/// Fixed alias table (design.md D6).
const ALIASES: &[(&str, &str)] = &[("WARNING", "WARN"), ("ERR", "ERROR"), ("CRITICAL", "FATAL")];

/// The eight RFC 5424 severity names folded onto the six-level table
/// (`prefix-and-payload-parsing` design.md D5). Kept apart from `ALIASES` because
/// these words — `alert`, `notice`, `emerg` — are ordinary English that appears in
/// message text, and `scan_message` must not turn a sentence mentioning "notice"
/// into an INFO entry. They resolve a *value* (a dedicated field, a positional
/// token, a numeric priority), never free text.
const SEVERITY_ALIASES: &[(&str, &str)] = &[
    ("EMERG", "FATAL"),
    ("PANIC", "FATAL"),
    ("ALERT", "FATAL"),
    ("CRIT", "FATAL"),
    ("NOTICE", "INFO"),
];

/// The eight syslog severities in RFC 5424 order, mapped onto the six levels (D5).
/// Widening `Level` to eight variants would touch filtering, the timeline's colour
/// mapping and the URL grammar for two severities no other format produces.
const SYSLOG_SEVERITIES: [Level; 8] = [
    Level::Fatal, // 0 emerg
    Level::Fatal, // 1 alert
    Level::Fatal, // 2 crit
    Level::Error, // 3 err
    Level::Warn,  // 4 warning
    Level::Info,  // 5 notice
    Level::Info,  // 6 info
    Level::Debug, // 7 debug
];

/// klog/logcat single-letter codes (D5). Accepted only from the positional slot —
/// `is_letter_code`/`from_letter` are never reachable from `scan_message`, because a
/// bare `E` in the middle of a sentence means nothing.
const LETTER_CODES: &[(u8, Level)] = &[
    (b'V', Level::Trace),
    (b'T', Level::Trace),
    (b'D', Level::Debug),
    (b'I', Level::Info),
    (b'W', Level::Warn),
    (b'E', Level::Error),
    (b'F', Level::Fatal),
];

/// Normalise a raw level *value* (a dedicated field, a positional token, a syslog
/// severity name) against the alias tables and the canonical set. Returns `None` for
/// anything that does not resolve to a known level — callers must not substitute a
/// default.
pub fn normalize(raw: &str) -> Option<Level> {
    let upper = raw.trim().to_ascii_uppercase();
    let canonical = ALIASES
        .iter()
        .chain(SEVERITY_ALIASES)
        .find(|(alias, _)| *alias == upper)
        .map(|(_, canonical)| *canonical)
        .unwrap_or(upper.as_str());
    Level::from_canonical(canonical)
}

/// Scan free text for the first case-insensitive match of a canonical level word or
/// one of its long-standing aliases, used by the plain-text parser and as the
/// JSON/logfmt fallback when no dedicated level field is present. Deliberately blind
/// to the syslog severity names and the single-letter codes (see `SEVERITY_ALIASES`
/// and `LETTER_CODES`).
///
/// A token immediately followed by `=` is skipped: it is a *key*, not a level
/// (`logcat-and-payload-precision` design.md D5). Without that guard
/// `… I incidentd: Done taking incident report err=Success` reported ERROR — the
/// `ERR` → `ERROR` alias firing on a key — on a line whose own severity letter was
/// `I`; 51 lines in the measured bugreport hit it. The fix has to be positional
/// rather than vocabulary-based: dropping `ERR` from the table instead would break
/// `level=err`, where `err` genuinely is the level. The difference between the two
/// cases is exactly which side of the `=` the token sits on.
pub fn scan_message(text: &str) -> Option<Level> {
    // Word-boundary scan without pulling in `regex` for a fixed, small vocabulary:
    // walk runs of ASCII letters and test each against the alias/canonical tables.
    // Cheaper than a regex crate dependency for this one job, and this module has no
    // other reason to depend on `regex`. Byte indices rather than `split` because the
    // rule above needs to know the character *after* the token, which `split`
    // discards; a run only ever starts and ends on an ASCII byte, so both ends are on
    // a `str` char boundary and multibyte text separates tokens exactly as before.
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'=') {
            continue; // a key, not a level
        }
        let upper = text[start..i].to_ascii_uppercase();
        let canonical = ALIASES
            .iter()
            .find(|(alias, _)| *alias == upper)
            .map(|(_, canonical)| *canonical)
            .unwrap_or(upper.as_str());
        if let Some(level) = Level::from_canonical(canonical) {
            return Some(level);
        }
    }
    None
}

/// Whether `b` is a klog/logcat severity letter. Used by the klog timestamp scanner,
/// which has to consume the letter *before* the timestamp it precedes.
pub fn is_letter_code(b: u8) -> bool {
    LETTER_CODES.iter().any(|(code, _)| *code == b)
}

/// A klog/logcat severity letter as a level.
pub fn from_letter(b: u8) -> Option<Level> {
    LETTER_CODES
        .iter()
        .find(|(code, _)| *code == b)
        .map(|(_, level)| *level)
}

/// A syslog PRI value (`<130>` = facility 16 × 8 + severity 2) as a level: the
/// severity is the low three bits, folded onto the six-level table.
pub fn from_syslog_priority(priority: u32) -> Level {
    SYSLOG_SEVERITIES[(priority % 8) as usize]
}

/// The token immediately after a recognised timestamp prefix, tested as a level
/// (design.md D5): bare (`INFO`), bracketed (`[INFO]`, `<INFO>`), suffixed (`INFO:`),
/// a syslog priority (`<130>`), or a single-letter klog/logcat code. Returns the
/// level and the rest of the line after the consumed token; `None` leaves the line
/// untouched for the whole-message scan to look at.
pub fn match_positional(rest: &str) -> Option<(Level, &str)> {
    let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    if token_end == 0 {
        return None;
    }
    let level = from_positional_token(&rest[..token_end])?;
    Some((level, rest[token_end..].trim_start()))
}

fn from_positional_token(token: &str) -> Option<Level> {
    let bytes = token.as_bytes();
    let (inner, angle_bracketed) = match (bytes.first(), bytes.last()) {
        (Some(b'['), Some(b']')) => (&token[1..token.len() - 1], false),
        (Some(b'<'), Some(b'>')) => (&token[1..token.len() - 1], true),
        _ => (token, false),
    };
    let inner = inner.trim_end_matches([':', ',']);
    if inner.is_empty() {
        return None;
    }
    // `<130>`: a numeric syslog priority rather than a level word.
    if angle_bracketed && inner.bytes().all(|b| b.is_ascii_digit()) {
        return inner.parse::<u32>().ok().map(from_syslog_priority);
    }
    if inner.len() == 1 {
        return from_letter(inner.as_bytes()[0]);
    }
    normalize(inner)
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

    // --- positional matching (task 4) ---------------------------------------

    #[test]
    fn positional_bare_bracketed_and_suffixed_forms() {
        assert_eq!(
            match_positional("INFO retrying"),
            Some((Level::Info, "retrying"))
        );
        assert_eq!(match_positional("[WARN] slow"), Some((Level::Warn, "slow")));
        assert_eq!(
            match_positional("<ERROR> boom"),
            Some((Level::Error, "boom"))
        );
        assert_eq!(
            match_positional("DEBUG: cache lookup"),
            Some((Level::Debug, "cache lookup"))
        );
    }

    #[test]
    fn positional_single_letters() {
        assert_eq!(match_positional("E boom"), Some((Level::Error, "boom")));
        assert_eq!(
            match_positional("V trace me"),
            Some((Level::Trace, "trace me"))
        );
    }

    #[test]
    fn positional_syslog_priority_maps_onto_the_six_levels() {
        // <130> = facility 16, severity 2 (crit) → FATAL.
        assert_eq!(match_positional("<130> disk"), Some((Level::Fatal, "disk")));
        // severity 6 (info)
        assert_eq!(
            match_positional("<134> hello"),
            Some((Level::Info, "hello"))
        );
    }

    #[test]
    fn positional_non_level_token_is_left_alone() {
        assert_eq!(match_positional("host app: boom"), None);
        assert_eq!(match_positional(""), None);
        assert_eq!(match_positional("[] boom"), None);
    }

    #[test]
    fn syslog_severity_names_resolve_as_values_but_not_in_prose() {
        assert_eq!(normalize("crit"), Some(Level::Fatal));
        assert_eq!(normalize("notice"), Some(Level::Info));
        assert_eq!(normalize("alert"), Some(Level::Fatal));
        // …but the whole-message scan stays blind to them, so an English sentence
        // mentioning them is not silently levelled.
        assert_eq!(
            scan_message("please notice the alert on the dashboard"),
            None
        );
    }

    #[test]
    fn single_letter_is_not_reachable_from_the_message_scan() {
        assert_eq!(scan_message("the value of E is unknown"), None);
    }

    // --- keys are not levels (`logcat-and-payload-precision` task 5.3) -------

    /// The measured defect: `err=Success` resolved through the `ERR` → `ERROR` alias
    /// and reported ERROR on a line whose own severity was INFO.
    #[test]
    fn a_key_is_not_a_level() {
        assert_eq!(
            scan_message("Done taking incident report err=Success"),
            None
        );
        assert_eq!(scan_message("warn=0 error=0 fatal=0"), None);
    }

    /// The other side of the same rule: a token on the *value* side of `=` is
    /// untouched, so a level written as a field value still resolves.
    #[test]
    fn a_level_on_the_value_side_still_resolves() {
        assert_eq!(scan_message("level=err"), Some(Level::Error));
        assert_eq!(scan_message("severity=warning retrying"), Some(Level::Warn));
        // …and `normalize`, which handles a *dedicated* field, is unaffected.
        assert_eq!(normalize("err"), Some(Level::Error));
    }

    #[test]
    fn the_key_rule_does_not_disturb_ordinary_prose() {
        assert_eq!(
            scan_message("ERROR: connection refused"),
            Some(Level::Error)
        );
        assert_eq!(scan_message("a=1 then WARN happened"), Some(Level::Warn));
        // Multibyte text still separates tokens the way `split` did.
        assert_eq!(scan_message("привет ERROR мир"), Some(Level::Error));
    }
}
