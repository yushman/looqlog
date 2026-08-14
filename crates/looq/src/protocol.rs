//! The `/ws` wire envelope (`stdin-stream` spec, design.md D1).
//!
//! Every message sent to a client is a JSON object tagged by `type`. Raw unlabelled
//! line text is never sent — a log line whose own text happens to look like a
//! serialised envelope is still carried inside a real envelope's `text` field, so it
//! is delivered as content, never interpreted (spec: "A line containing envelope-like
//! text is not misread").
//!
//! D1 picked JSON over a binary framing: inspectable with `wscat`/any WS client,
//! which is how every test in this change is written, at the cost of some parsing
//! overhead that is not expected to matter at the volumes this change targets (the
//! ring buffer is already shedding load before envelope parsing would show up in a
//! profile — recorded as the revisit trigger).

use serde::{Deserialize, Serialize};

/// The only message a client is allowed to send, and only as the very first frame
/// (`security` spec, design.md D1). The token travels here, inside the WebSocket
/// payload — never in the connection URL/query string, so it can't end up in shell
/// history or a proxy access log. A connection that does not present a matching
/// token within the server's handshake timeout is closed before any stdin data is
/// sent (`server.rs`'s `authenticate`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Auth { token: String },
}

/// One buffered line, carried inside a [`ServerMessage::Snapshot`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SnapshotLine {
    pub seq: u64,
    pub text: String,
}

/// Every message the backend sends over `/ws`, tagged by `type` (D1). `line` and
/// `snapshot` both carry sequence numbers (D2) so a client can detect a gap by
/// discontinuity even if the `gap` message that describes it is itself dropped by
/// the same backpressure that caused it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage<'a> {
    /// Sent once per new/reloaded connection, before any `Line`. Carries the current
    /// ring buffer contents and the sequence number of its last line so the client
    /// can position itself in the stream (D3) and dedup a reconnect.
    Snapshot {
        lines: &'a [SnapshotLine],
        last_seq: u64,
    },
    /// A single live line.
    Line { seq: u64, text: &'a str },
    /// Lines were dropped for this client under backpressure (per-client bounded
    /// channel, drop-oldest). `count` is exact, not an estimate.
    Gap { count: u64 },
    /// stdin closed; no more `Line`/`Gap` messages will follow on this connection.
    Ended,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_message_uses_camel_case_fields_and_snake_case_tag() {
        let msg = ServerMessage::Line {
            seq: 7,
            text: "hello",
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"line","seq":7,"text":"hello"}"#);
    }

    #[test]
    fn snapshot_message_carries_last_seq_camel_case() {
        let lines = vec![
            SnapshotLine {
                seq: 1,
                text: "a".into(),
            },
            SnapshotLine {
                seq: 2,
                text: "b".into(),
            },
        ];
        let msg = ServerMessage::Snapshot {
            lines: &lines,
            last_seq: 2,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"snapshot","lines":[{"seq":1,"text":"a"},{"seq":2,"text":"b"}],"lastSeq":2}"#
        );
    }

    #[test]
    fn gap_message_carries_count() {
        let msg = ServerMessage::Gap { count: 42 };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"gap","count":42}"#);
    }

    #[test]
    fn ended_message_has_only_a_type_tag() {
        let msg = ServerMessage::Ended;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"ended"}"#);
    }

    #[test]
    fn client_auth_message_round_trips() {
        let json = r#"{"type":"auth","token":"deadbeef"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            ClientMessage::Auth {
                token: "deadbeef".to_string()
            }
        );
    }

    /// task 1.3: a log line whose own text looks like a serialised envelope must
    /// still be delivered as an ordinary `line` message with that text as an
    /// opaque string value — never interpreted as a second, nested envelope.
    #[test]
    fn a_line_that_looks_like_an_envelope_is_carried_as_opaque_text() {
        let tricky = r#"{"type":"gap","count":999}"#;
        let msg = ServerMessage::Line {
            seq: 1,
            text: tricky,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "line");
        assert_eq!(parsed["text"], tricky);
        // The outer message is a `line`; `tricky`'s own `"type":"gap"` only exists
        // inside the string value of `text`, never as a sibling key.
        assert_eq!(parsed.as_object().unwrap().len(), 3); // type, seq, text
    }
}
