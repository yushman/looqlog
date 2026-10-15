#!/usr/bin/env bash
# Rebuilds the vendored frontend artifacts in crates/looqlog/assets/ from web/ sources
# (ADR-0008). This is the single documented command referenced by the `packaging`
# spec and CI's staleness check.
#
# The artifacts live inside crates/looqlog/ (not the repo root) because `cargo
# package`/`cargo publish` only ships files under the crate directory being
# published — anything referenced via include_bytes!/include_str! with a path that
# escapes the crate root would silently be missing from the published tarball.
#
# Requires: Node.js + npm (only to build web/ — the published crate itself needs
# neither, `packaging` spec "Frontend sources are not shipped to end users").
#
# `vite build` compiles web/src into dist/assets/{index.js,index.css} with fixed
# (unhashed) filenames, so a source-only change reliably touches only the files it
# affects, and two runs with no source change are byte-identical. No WebAssembly is
# built or vendored (ADR-0010).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ASSETS_DIR="$ROOT/crates/looqlog/assets"
WEB_DIR="$ROOT/web"

# Node is an input to the diffed bytes through Vite; CI pins its major version in the
# frontend-artifact-staleness job. Vite itself is pinned exactly in web/package.json.
EXPECTED_NODE_MAJOR=24
have_node=$(node --version | sed 's/^v//')
if [ "${have_node%%.*}" != "$EXPECTED_NODE_MAJOR" ]; then
  echo "warning: Node.js $have_node, expected $EXPECTED_NODE_MAJOR.x — the rebuilt" >&2
  echo "         artifacts may differ from CI's and fail the staleness check." >&2
fi

( cd "$WEB_DIR" && npm ci && npm run build )

rm -rf "$ASSETS_DIR"
mkdir -p "$ASSETS_DIR"
cp -R "$WEB_DIR/dist/." "$ASSETS_DIR/"

# `crates/looqlog/src/assets.rs` embeds these files at compile time via
# include_str!/include_bytes!. A `cargo build` right after this script, with only the
# asset *contents* changed, sometimes did not recompile `assets.rs` and kept serving
# the previous build's embedded bytes. Touching the source file forces rustc to
# reprocess it on the next build. Cheap; always do it.
touch "$ROOT/crates/looqlog/src/assets.rs"

echo "rebuilt crates/looqlog/assets/ from web/ (index.html, assets/{index.js,index.css})"
