/** Half-open time range in epoch milliseconds. Shell-owned state (design.md D7):
 * the timeline produces it, the table consumes it, and the shell (`looq-app.ts` in
 * file mode, `looq-live-tail.ts` in stream mode) is what passes it between them —
 * neither component holds or wires to the other directly. */
export interface TimeRange {
  startMs: number;
  endMs: number;
}
