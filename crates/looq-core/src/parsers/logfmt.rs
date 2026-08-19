//! logfmt parser (log-parsing spec, "logfmt parsing"). Never reports a line as
//! malformed: a line with no `key=value` pairs at all still becomes an entry whose
//! message is the bare text, the same way plain text does.

use std::collections::BTreeMap;

use super::{extract, Extracted};
use crate::entry::FieldValue;
use crate::timestamp::ParseContext;

/// One token from a logfmt line: a `key=value` pair, or a bare word.
enum Token {
    Pair(String, String),
    Bare(String),
}

/// Consume a `{…}` block starting at `start` (which must be the `{`) and return the
/// index one past its matching `}` — tracking nesting depth and ignoring braces
/// inside quoted spans (`logcat-and-payload-precision` design.md D4).
///
/// This is what stops a Java map dump inside a log line from becoming a field per
/// member. Without it, `headers={null=[HTTP/1.1 204], Alt-Svc=[h3], Content-Length=[0]}`
/// contributed top-level chips named `Alt-Svc` and `Content-Length`; with it the whole
/// block is one `headers` value, which is the same treatment nested JSON already gets
/// (`prefix-and-payload-parsing` design.md D8: nested structures are kept as their
/// text, not flattened).
///
/// An unterminated block consumes to end of line rather than failing: a truncated line
/// is not a malformed one under this parser, and there is nothing better to do with
/// the remainder than keep it as the value.
fn skip_braced(chars: &[char], start: usize) -> usize {
    let mut k = start;
    let mut depth = 0usize;
    let mut in_quotes = false;
    while k < chars.len() {
        let c = chars[k];
        if in_quotes {
            if c == '\\' {
                k = (k + 2).min(chars.len());
                continue;
            }
            if c == '"' {
                in_quotes = false;
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == '{' {
            depth += 1;
        } else if c == '}' {
            // `depth` is at least 1 here: the caller guarantees `chars[start] == '{'`,
            // which is counted on the first iteration.
            depth -= 1;
            if depth == 0 {
                return k + 1;
            }
        }
        k += 1;
    }
    chars.len()
}

/// Split a line into `key=value` pairs and bare tokens, honouring double-quoted
/// values that may contain spaces and backslash-escaped characters, and consuming a
/// brace-delimited value as one opaque block (design.md D4).
fn tokenize(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        let mut j = i;
        let mut eq_pos = None;
        while j < chars.len() && !chars[j].is_whitespace() {
            if chars[j] == '=' && eq_pos.is_none() {
                eq_pos = Some(j);
                break;
            }
            j += 1;
        }

        if let Some(eq) = eq_pos {
            let key: String = chars[start..eq].iter().collect();
            let mut k = eq + 1;
            let value = if k < chars.len() && chars[k] == '"' {
                k += 1;
                let mut buf = String::new();
                while k < chars.len() {
                    if chars[k] == '\\' && k + 1 < chars.len() {
                        buf.push(chars[k + 1]);
                        k += 2;
                    } else if chars[k] == '"' {
                        k += 1;
                        break;
                    } else {
                        buf.push(chars[k]);
                        k += 1;
                    }
                }
                buf
            } else if k < chars.len() && chars[k] == '{' {
                let vstart = k;
                k = skip_braced(&chars, k);
                chars[vstart..k].iter().collect()
            } else {
                let vstart = k;
                while k < chars.len() && !chars[k].is_whitespace() {
                    k += 1;
                }
                chars[vstart..k].iter().collect()
            };
            tokens.push(Token::Pair(key, value));
            i = k;
        } else {
            let bare: String = chars[start..j].iter().collect();
            tokens.push(Token::Bare(bare));
            i = j;
        }
    }

    tokens
}

/// A line always becomes an entry under logfmt: `key=value` pairs are extracted as
/// fields, bare tokens are joined (in order) as the fallback message when no
/// recognised message key is present.
pub fn parse_line(line: &str, ctx: &ParseContext) -> Extracted {
    let mut raw: BTreeMap<String, FieldValue> = BTreeMap::new();
    let mut bare_parts = Vec::new();

    for token in tokenize(line) {
        match token {
            Token::Pair(key, value) => {
                raw.insert(key, FieldValue::String(value));
            }
            Token::Bare(word) => bare_parts.push(word),
        }
    }

    let mut extracted = extract(raw, ctx);
    if extracted.message.is_none() {
        extracted.message = if bare_parts.is_empty() {
            Some(String::new())
        } else {
            Some(bare_parts.join(" "))
        };
    }
    extracted
}

/// Whether `line` looks like logfmt, for format detection: at least half its
/// whitespace-separated tokens are `key=value` pairs, and at least one is. A pure
/// substring/`=`-count check would false-positive on free text containing a stray
/// `=`; requiring a majority of tokens to be pair-shaped is the judgement call
/// documented alongside the detection threshold in design.md D3.
pub fn matches(line: &str) -> bool {
    let tokens = tokenize(line);
    if tokens.is_empty() {
        return false;
    }
    let pairs = tokens
        .iter()
        .filter(|t| matches!(t, Token::Pair(_, _)))
        .count();
    pairs >= 1 && (pairs as f64 / tokens.len() as f64) >= 0.5
}

/// Minimum `key=value` pairs a plain-text payload must carry before it is handed to
/// this parser (design.md D6). Two rather than one for the same reason detection uses
/// 80% rather than "any match": English prose contains `foo=bar` far more often than
/// it contains two of them, and a false positive here silently turns message text
/// into filter chips.
const PAYLOAD_MIN_PAIRS: usize = 2;

/// Whether the text behind a recognised prefix looks like a logfmt payload. The `=`
/// count is a cheap reject that keeps the tokenizer off the hot path for the vast
/// majority of plain-text lines, which contain no `=` at all.
pub fn looks_like_payload(line: &str) -> bool {
    if line.bytes().filter(|b| *b == b'=').count() < PAYLOAD_MIN_PAIRS {
        return false;
    }
    tokenize(line)
        .iter()
        .filter(|t| matches!(t, Token::Pair(_, _)))
        .count()
        >= PAYLOAD_MIN_PAIRS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_value_with_spaces() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"level=error msg="connection refused" service=api"#, &tz);
        assert_eq!(extracted.message.as_deref(), Some("connection refused"));
        match extracted.fields.get("service").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "api"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn bare_tokens_become_message() {
        let tz = ParseContext::utc();
        let extracted = parse_line("starting up level=info", &tz);
        assert_eq!(extracted.message.as_deref(), Some("starting up"));
        assert_eq!(
            extracted.level,
            Some(crate::level::normalize("info").unwrap())
        );
    }

    #[test]
    fn escaped_quote_in_value() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"msg="she said \"hi\"""#, &tz);
        assert_eq!(extracted.message.as_deref(), Some(r#"she said "hi""#));
    }

    #[test]
    fn matches_true_for_kv_heavy_line() {
        assert!(matches("level=error msg=\"x\" service=api"));
    }

    #[test]
    fn matches_false_for_free_text() {
        assert!(!matches("just some free text with no pairs at all"));
    }

    // --- braced values (`logcat-and-payload-precision` tasks 4.2, 4.3) -------

    fn field_names(extracted: &Extracted) -> Vec<&str> {
        extracted.fields.keys().map(String::as_str).collect()
    }

    /// The measured case, from the Android bugreport that motivated the fix: a Java
    /// map dump used to contribute every one of its members as a top-level field, so
    /// the UI grew filter chips named `Alt-Svc` and `Content-Length`.
    #[test]
    fn braced_value_is_one_field_not_many() {
        let tz = ParseContext::utc();
        let extracted = parse_line(
            "time=18ms ret=204 headers={null=[HTTP/1.1 204], Alt-Svc=[h3], Content-Length=[0]}",
            &tz,
        );
        assert_eq!(field_names(&extracted), vec!["headers", "ret", "time"]);
        match extracted.fields.get("headers").unwrap() {
            FieldValue::String(s) => {
                assert_eq!(s, "{null=[HTTP/1.1 204], Alt-Svc=[h3], Content-Length=[0]}")
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn sibling_pairs_on_both_sides_of_a_braced_value_survive() {
        let tz = ParseContext::utc();
        let extracted = parse_line("a=1 request={x=[1], y=[2]} b=2", &tz);
        assert_eq!(field_names(&extracted), vec!["a", "b", "request"]);
        match extracted.fields.get("b").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "2"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn nested_braces_are_balanced_to_the_outer_one() {
        let tz = ParseContext::utc();
        let extracted = parse_line("outer={a={b=1}, c=2} after=yes", &tz);
        assert_eq!(field_names(&extracted), vec!["after", "outer"]);
        match extracted.fields.get("outer").unwrap() {
            // Not truncated at the first `}`.
            FieldValue::String(s) => assert_eq!(s, "{a={b=1}, c=2}"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_brace_inside_a_quoted_span_does_not_change_the_depth() {
        let tz = ParseContext::utc();
        let extracted = parse_line(r#"v={a="}", b=2} after=yes"#, &tz);
        assert_eq!(field_names(&extracted), vec!["after", "v"]);
        match extracted.fields.get("v").unwrap() {
            FieldValue::String(s) => assert_eq!(s, r#"{a="}", b=2}"#),
            other => panic!("unexpected {other:?}"),
        }
    }

    /// A truncated line is not a malformed one: the block consumes to end of line
    /// rather than hanging, panicking or dropping the pair.
    #[test]
    fn unterminated_brace_consumes_to_end_of_line() {
        let tz = ParseContext::utc();
        let extracted = parse_line("a=1 rest={x=[1], y=[2]", &tz);
        assert_eq!(field_names(&extracted), vec!["a", "rest"]);
        match extracted.fields.get("rest").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "{x=[1], y=[2]"),
            other => panic!("unexpected {other:?}"),
        }
        // An unterminated quote inside an unterminated block terminates too.
        let extracted = parse_line(r#"a=1 rest={x="unclosed"#, &tz);
        assert_eq!(field_names(&extracted), vec!["a", "rest"]);
    }

    #[test]
    fn payload_threshold_needs_two_pairs() {
        assert!(!looks_like_payload("retrying because mode=safe was set"));
        assert!(looks_like_payload("level=error msg=\"boom\" service=api"));
        // A quoted value containing `=` does not lift a one-pair line over the bar.
        assert!(!looks_like_payload(r#"msg="a=b=c""#));
    }
}
