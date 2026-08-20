// Mirror of `crates/looqlog/src/protocol.rs`'s `ServerMessage` (`stdin-stream` spec,
// design.md D1). Hand-written like `wasm-types.ts`, kept in sync by convention;
// `tsc --noEmit` catches stale field usages left behind by a rename on either side.

export interface SnapshotLineDto {
  seq: number;
  text: string;
}

export type ServerMessageDto =
  | { type: "snapshot"; lines: SnapshotLineDto[]; lastSeq: number }
  | { type: "line"; seq: number; text: string }
  | { type: "gap"; count: number }
  | { type: "ended" };
