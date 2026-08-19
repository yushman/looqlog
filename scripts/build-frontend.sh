#!/usr/bin/env bash
# Rebuilds the vendored frontend artifacts in crates/looq/assets/ from web/ sources
# and crates/looq-wasm (ADR-0008). This is the single documented command referenced
# by the `packaging` spec and CI's staleness check.
#
# The artifacts live inside crates/looq/ (not the repo root) because `cargo
# package`/`cargo publish` only ships files under the crate directory being
# published — anything referenced via include_bytes!/include_str! with a path that
# escapes the crate root would silently be missing from the published tarball.
#
# Requires: wasm-pack, a Rust toolchain with the wasm32-unknown-unknown target,
# Node.js + npm (only to build web/ — the published crate itself needs neither,
# `packaging` spec "Frontend sources are not shipped to end users").
#
# Two builds (wasm-pack, then vite) feed into one output tree:
#   1. wasm-pack builds crates/looq-wasm --target web into web/public/wasm/, which
#      Vite then copies byte-for-byte into dist/wasm/ (public/ assets pass through
#      unhashed and untransformed — see web/vite.config.ts).
#   2. vite build compiles web/src into dist/assets/{index.js,index.css,worker.js}
#      with fixed (unhashed) filenames, so a source-only change reliably touches
#      only the files it affects — verified byte-identical across two consecutive
#      runs with no source change (browser-app-shell task 1.3; see docs/devlog.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM_TMP="$ROOT/target/frontend-build-tmp"
ASSETS_DIR="$ROOT/crates/looq/assets"
WEB_DIR="$ROOT/web"

# rustc embeds absolute source paths in panic messages, so a wasm built here and a
# wasm built on a CI runner differed by the length of `/Users/<name>` vs
# `/home/runner` — 4 bytes at last measurement, enough to fail the staleness diff
# forever while nothing was actually stale. Remapping makes the output identical
# across machines, and stops the builder's username from shipping inside the
# released binaries (the artifacts are embedded via include_bytes!).
export RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${ROOT}=/looq ${RUSTFLAGS:-}"

# The wasm-opt build is part of the artifact, so a different binaryen produces a
# different core.wasm and the staleness check fails with no source change. CI pins
# this version; warn loudly here rather than let the mismatch surface as a
# confusing red CI run on someone else's push.
EXPECTED_BINARYEN=132
if command -v wasm-opt >/dev/null 2>&1; then
  have=$(wasm-opt --version | awk '{print $3}')
  if [ "$have" != "$EXPECTED_BINARYEN" ]; then
    echo "warning: wasm-opt version $have, expected $EXPECTED_BINARYEN — the rebuilt" >&2
    echo "         core.wasm will differ from CI's and fail the staleness check." >&2
  fi
else
  echo "warning: no wasm-opt on PATH; wasm-pack will download its own, which may" >&2
  echo "         not be version $EXPECTED_BINARYEN — see above." >&2
fi

rm -rf "$WASM_TMP"
wasm-pack build "$ROOT/crates/looq-wasm" \
  --target web \
  --out-dir "$WASM_TMP" \
  --out-name core \
  --release \
  --no-typescript \
  --no-pack

mkdir -p "$WEB_DIR/public/wasm"
cp "$WASM_TMP/core.js" "$WEB_DIR/public/wasm/core.js"
cp "$WASM_TMP/core_bg.wasm" "$WEB_DIR/public/wasm/core.wasm"
rm -rf "$WASM_TMP"

( cd "$WEB_DIR" && npm ci && npm run build )

rm -rf "$ASSETS_DIR"
mkdir -p "$ASSETS_DIR"
cp -R "$WEB_DIR/dist/." "$ASSETS_DIR/"

# `crates/looq/src/assets.rs` embeds these files at compile time via
# include_str!/include_bytes!. Observed during release-hardening: a `cargo build`
# run shortly after this script, with only the asset *contents* changed (same
# paths, same mtimes-adjacent-enough window), sometimes did not recompile
# `assets.rs` and kept serving the previous build's embedded bytes — caught only by
# diffing the running server's response against the file on disk, not by any
# compiler warning. Touching the source file itself (not just the assets it
# includes) forces rustc to reprocess it on the next build, regardless of the
# fingerprinting question above. Cheap; always do it.
touch "$ROOT/crates/looq/src/assets.rs"

echo "rebuilt crates/looq/assets/ from web/ (index.html, assets/{index.js,index.css,worker.js}, wasm/{core.js,core.wasm})"
