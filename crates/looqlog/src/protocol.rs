//! The `/ws` wire envelope (`query-api`/`stdin-stream` specs, ADR-0010).
//!
//! Every frame is a JSON object tagged by `type`. The client authenticates first, then
//! sends requests carrying an `id`; each gets exactly one `reply` or `error` with the
//! same `id`. The server also pushes untagged notifications. Log text only ever travels
//! inside a reply's row fields, so a line that looks like an envelope is carried as
//! content, never interpreted.

use std::collections::BTreeMap;

use looqlog_core::dto::TimeRangeDto;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ClientMessage {
    /// Must be the very first frame; the token never appears in the URL.
    Auth {
        token: String,
    },
    /// The whole view state. Starts a new generation unless the search fails to compile.
    Query {
        id: u64,
        #[serde(default)]
        filters: BTreeMap<String, Vec<String>>,
        #[serde(default)]
        search: String,
        #[serde(default)]
        range: Option<TimeRangeDto>,
        #[serde(default)]
        toggled: Option<Vec<usize>>,
    },
    /// `anchor`: start at the visible position of this ordinal instead of `offset`.
    Window {
        id: u64,
        #[serde(default)]
        offset: usize,
        limit: usize,
        #[serde(default)]
        anchor: Option<usize>,
    },
    Entry {
        id: u64,
        ordinal: usize,
    },
    Buckets {
        id: u64,
        #[serde(default)]
        span: Option<TimeRangeDto>,
        #[serde(default)]
        full_span: bool,
    },
    /// `format`: `"json" | "logfmt" | "plain"`, or `null`/`"auto"` for detection.
    SetFormat {
        id: u64,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        tz_offset_minutes: Option<i32>,
    },
    ToggleChain {
        id: u64,
        root: usize,
    },
    /// Source description, detection, diagnostics and field inventory.
    Stats {
        id: u64,
    },
}

impl ClientMessage {
    pub fn id(&self) -> Option<u64> {
        match self {
            ClientMessage::Auth { .. } => None,
            ClientMessage::Query { id, .. }
            | ClientMessage::Window { id, .. }
            | ClientMessage::Entry { id, .. }
            | ClientMessage::Buckets { id, .. }
            | ClientMessage::SetFormat { id, .. }
            | ClientMessage::ToggleChain { id, .. }
            | ClientMessage::Stats { id } => Some(*id),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    Reply {
        id: u64,
        data: serde_json::Value,
    },
    Error {
        id: u64,
        message: String,
    },
    /// Bytes of the file ingested so far out of its size (file sources only).
    Progress {
        bytes: u64,
        total: u64,
        done: bool,
    },
    /// Cumulative counts after ingest: retained entries, and this connection's matched
    /// entries and visible rows.
    Grown {
        total: usize,
        matched: usize,
        row_count: usize,
    },
    /// Cumulative number of lines dropped from retention.
    Evicted {
        count: u64,
    },
    /// stdin closed; retained entries stay queryable.
    Ended,
    /// The source was replaced or re-parsed; the client must re-query.
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_request_round_trips_with_defaults() {
        let msg: ClientMessage =
            serde_json::from_str(r#"{"type":"query","id":3,"search":"timeout"}"#).unwrap();
        assert_eq!(
            msg,
            ClientMessage::Query {
                id: 3,
                filters: BTreeMap::new(),
                search: "timeout".into(),
                range: None,
                toggled: None,
            }
        );
        assert_eq!(msg.id(), Some(3));
    }

    #[test]
    fn camel_case_request_fields() {
        let msg: ClientMessage = serde_json::from_str(
            r#"{"type":"set_format","id":1,"format":"logfmt","tzOffsetMinutes":120}"#,
        )
        .unwrap();
        assert_eq!(
            msg,
            ClientMessage::SetFormat {
                id: 1,
                format: Some("logfmt".into()),
                tz_offset_minutes: Some(120),
            }
        );
        let msg: ClientMessage = serde_json::from_str(
            r#"{"type":"buckets","id":2,"span":{"startMs":0,"endMs":10},"fullSpan":true}"#,
        )
        .unwrap();
        assert!(matches!(
            msg,
            ClientMessage::Buckets {
                full_span: true,
                span: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn auth_message_round_trips() {
        let msg: ClientMessage =
            serde_json::from_str(r#"{"type":"auth","token":"deadbeef"}"#).unwrap();
        assert_eq!(
            msg,
            ClientMessage::Auth {
                token: "deadbeef".into()
            }
        );
    }

    #[test]
    fn notifications_are_tagged_and_camel_case() {
        let json = serde_json::to_string(&ServerMessage::Grown {
            total: 3,
            matched: 2,
            row_count: 1,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"grown","total":3,"matched":2,"rowCount":1}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerMessage::Ended).unwrap(),
            r#"{"type":"ended"}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerMessage::Evicted { count: 50 }).unwrap(),
            r#"{"type":"evicted","count":50}"#
        );
    }

    /// A log line whose own text looks like an envelope stays an opaque string inside
    /// the reply's data — never a sibling key of the outer message.
    #[test]
    fn a_line_that_looks_like_an_envelope_is_carried_as_opaque_text() {
        let tricky = r#"{"type":"reset"}"#;
        let msg = ServerMessage::Reply {
            id: 1,
            data: serde_json::json!({ "rows": [{ "message": tricky }] }),
        };
        let parsed: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(parsed["type"], "reply");
        assert_eq!(parsed["data"]["rows"][0]["message"], tricky);
        assert_eq!(parsed.as_object().unwrap().len(), 3);
    }
}
