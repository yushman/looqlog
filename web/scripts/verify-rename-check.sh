#!/usr/bin/env bash
# Proves the CI type check actually catches a Rust-side field rename (wasm-bridge
# spec, "Type check catches a shape change"; browser-app-shell task 2.2).
#
# What "catches" means here, precisely (design.md D2 open question, resolved in
# docs/devlog.md: hand-written types, not tsify-generated): the TS interfaces in
# wasm-types.ts are a hand-maintained mirror of crates/looqlog-wasm/src/dto.rs, not
# generated from it, so `tsc` cannot detect a Rust-side rename directly — it has no
# view of the Rust source. What it CAN and DOES catch is the failure mode D2
# actually describes: a field is renamed (in Rust, and in this TS mirror, by the
# same developer, in the same change) but a usage site elsewhere in the frontend is
# missed. Without the type check, that stale usage silently reads `undefined` at
# runtime. With it, `tsc --noEmit` fails at the stale usage site immediately.
#
# This script simulates exactly that: renames `EntryDto.ordinal` to `lineOrdinal` in
# wasm-types.ts (the "interface got updated" half of the mistake) while leaving
# looqlog-entry-table.ts's `entry.ordinal` read untouched (the "usage site got missed"
# half), runs the real CI type-check command, asserts it fails naming the stale
# property, then restores both files from backups unconditionally (via a trap) so a
# crash mid-script never leaves the working tree broken.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TYPES_FILE="$ROOT/src/wasm-types.ts"
BACKUP="$(mktemp)"

cleanup() {
  cp "$BACKUP" "$TYPES_FILE"
  rm -f "$BACKUP"
}
trap cleanup EXIT

cp "$TYPES_FILE" "$BACKUP"

# Rename only the interface field, not the (deliberately left stale) usage in
# src/components/looqlog-entry-table.ts, which reads `entry.ordinal`.
sed -i.bak 's/  ordinal: number;/  lineOrdinal: number;/' "$TYPES_FILE"
rm -f "$TYPES_FILE.bak"

echo "== renamed EntryDto.ordinal -> lineOrdinal in wasm-types.ts, usage sites left as-is =="
echo "== running: npm run typecheck (expected to FAIL) =="

set +e
( cd "$ROOT" && npm run typecheck ) >/tmp/verify-rename-check.out 2>&1
STATUS=$?
set -e

cat /tmp/verify-rename-check.out

if [ "$STATUS" -eq 0 ]; then
  echo "FAIL: tsc --noEmit passed with a stale 'ordinal' usage after the interface renamed the field — the type check does not actually catch this class of bug."
  exit 1
fi

if ! grep -q "ordinal" /tmp/verify-rename-check.out; then
  echo "FAIL: tsc --noEmit failed, but not on the expected stale 'ordinal' property — check the output above."
  exit 1
fi

echo "OK: tsc --noEmit failed and named the stale 'ordinal' usage, as expected."
