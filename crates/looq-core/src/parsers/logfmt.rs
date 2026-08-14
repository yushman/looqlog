//! logfmt parser (log-parsing spec, "logfmt parsing"). Never reports a line as
//! malformed: a line with no `key=value` pairs at all still becomes an entry whose
//! message is the bare text, the same way plain text does.

use std::collections::BTreeMap;

use super::{extract, Extracted};
use crate::entry::FieldValue;
use crate::timestamp::TimeZonePolicy;

/// One token from a logfmt line: a `key=value` pair, or a bare word.
enum Token {
    Pair(String, String),
    Bare(String),
}

/// Split a line into `key=value` pairs and bare tokens, honouring double-quoted
/// values that may contain spaces and backslash-escaped characters.
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
pub fn parse_line(line: &str, tz: &TimeZonePolicy) -> Extracted {
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

    let mut extracted = extract(raw, tz);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_value_with_spaces() {
        let tz = TimeZonePolicy::utc();
        let extracted = parse_line(r#"level=error msg="connection refused" service=api"#, &tz);
        assert_eq!(extracted.message.as_deref(), Some("connection refused"));
        match extracted.fields.get("service").unwrap() {
            FieldValue::String(s) => assert_eq!(s, "api"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn bare_tokens_become_message() {
        let tz = TimeZonePolicy::utc();
        let extracted = parse_line("starting up level=info", &tz);
        assert_eq!(extracted.message.as_deref(), Some("starting up"));
        assert_eq!(
            extracted.level,
            Some(crate::level::normalize("info").unwrap())
        );
    }

    #[test]
    fn escaped_quote_in_value() {
        let tz = TimeZonePolicy::utc();
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
}
