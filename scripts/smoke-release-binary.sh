#!/usr/bin/env bash
# Smoke-tests a freshly built `looqlog` release binary on the machine that just built
# it — a machine with no development environment for this project (`packaging`
# spec, "Verification from a binary that never saw the dev machine"; design D3).
# Runs under `bash` on Linux, macOS and Git Bash (Windows).
#
# Usage: scripts/smoke-release-binary.sh <path-to-binary> <expected-version>
#
# NOTE ON MODE: this script never exercises file mode. `mode_for` in
# crates/looqlog/src/cli.rs selects stdin mode whenever stdin is not a TTY, and on a
# CI runner stdin is never a TTY — `< /dev/null` does not make it one. So
# `looqlog some.log` on a runner picks stdin mode regardless of the path argument
# (design D3). File mode is client-side by construction (ADR-0002/ADR-0007): the
# backend's entire file-mode duty is printing a hint string, and it never reads the
# file — a pty would add coverage of that hint string, not of parsing. So this
# script asserts what is genuinely reachable headlessly: the server serving the
# embedded UI, and stdin lines reaching a WebSocket client.

set -euo pipefail

BIN="${1:?usage: smoke-release-binary.sh <binary> <expected-version>}"
EXPECTED_VERSION="${2:?usage: smoke-release-binary.sh <binary> <expected-version>}"

# Fixed, not `--port 0` — the smoke test needs to know the port without parsing
# stdout.
PORT=47891
BASE_URL="http://127.0.0.1:${PORT}"
SMOKE_LINE="looqlog-smoke-test-line-$$"

PY="python3"
command -v python3 >/dev/null 2>&1 || PY="python"

fail() {
    echo "FAIL: $1" >&2
    echo "  expected: $2" >&2
    echo "  observed: $3" >&2
    exit 1
}

WORKDIR="$(mktemp -d)"
SERVER_PID=""
cleanup() {
    if [ -n "$SERVER_PID" ]; then
        kill "$SERVER_PID" >/dev/null 2>&1 || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

echo "== --version =="
ACTUAL_VERSION_LINE="$("$BIN" --version)" ||
    fail "--version exits 0" "a clean exit" "$BIN --version failed to run"
EXPECTED_VERSION_LINE="looqlog ${EXPECTED_VERSION}"
[ "$ACTUAL_VERSION_LINE" = "$EXPECTED_VERSION_LINE" ] ||
    fail "--version output" "$EXPECTED_VERSION_LINE" "$ACTUAL_VERSION_LINE"
echo "ok: $ACTUAL_VERSION_LINE"

echo "== --help =="
HELP_OUTPUT="$("$BIN" --help)" ||
    fail "--help exits 0" "a clean exit" "$BIN --help failed to run"
for flag in --port --host --open --no-browser --stdin --max-lines --version --help; do
    case "$HELP_OUTPUT" in
    *"$flag"*) ;;
    *) fail "--help names every TDR §6 flag" "$flag present" "$flag missing from --help output" ;;
    esac
done
echo "ok: --help names every flag"

echo "== starting server (stdin mode, one piped line) =="
echo "$SMOKE_LINE" | "$BIN" --stdin --port "$PORT" --no-browser \
    >"$WORKDIR/stdout.log" 2>"$WORKDIR/stderr.log" &
SERVER_PID=$!

READY=0
for _ in $(seq 1 50); do
    if curl -s -o /dev/null "$BASE_URL/"; then
        READY=1
        break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        break
    fi
    sleep 0.2
done
if [ "$READY" != "1" ]; then
    cat "$WORKDIR/stdout.log" "$WORKDIR/stderr.log" >&2 || true
    fail "server accepts connections within 10s" "listening on $BASE_URL" "server never came up (see log above)"
fi

echo "== GET / =="
INDEX_CODE=$(curl -s -o "$WORKDIR/index.html" -w "%{http_code}" "$BASE_URL/")
[ "$INDEX_CODE" = "200" ] || fail "GET / status" "200" "$INDEX_CODE"
[ -s "$WORKDIR/index.html" ] || fail "GET / body" "non-empty body" "empty body"
echo "ok: / returned 200 with a non-empty body"

echo "== GET /wasm/core.wasm =="
WASM_HEADERS=$(curl -s -D - -o "$WORKDIR/core.wasm" "$BASE_URL/wasm/core.wasm")
echo "$WASM_HEADERS" | grep -q "^HTTP/[0-9.]* 200" ||
    fail "GET /wasm/core.wasm status" "200" "$(echo "$WASM_HEADERS" | head -1)"
echo "$WASM_HEADERS" | grep -qi "^content-type: *application/wasm" ||
    fail "GET /wasm/core.wasm Content-Type" "application/wasm" "$(echo "$WASM_HEADERS" | grep -i '^content-type:' || echo '(missing)')"
[ -s "$WORKDIR/core.wasm" ] || fail "GET /wasm/core.wasm body" "non-empty body" "empty body"
echo "ok: /wasm/core.wasm returned 200 with application/wasm"

echo "== stdin line reaches a WebSocket client on /ws =="
TOKEN=$(sed -n 's/.*<template id="token-template">\([^<]*\)<\/template>.*/\1/p' "$WORKDIR/index.html")
[ -n "$TOKEN" ] || fail "token-template present in served page" "a non-empty per-process token" "no token found in GET / body"

if ! "$PY" - "$PORT" "$TOKEN" "$SMOKE_LINE" <<'PYEOF'
import base64
import json
import os
import socket
import struct
import sys
import time

host = "127.0.0.1"
port = int(sys.argv[1])
token = sys.argv[2]
expected_line = sys.argv[3]


def recv_exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise RuntimeError("connection closed while reading a frame")
        buf += chunk
    return buf


def ws_handshake(sock):
    key = base64.b64encode(os.urandom(16)).decode()
    req = (
        "GET /ws HTTP/1.1\r\n"
        f"Host: {host}:{port}\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        "Sec-WebSocket-Version: 13\r\n"
        "\r\n"
    )
    sock.sendall(req.encode())
    resp = b""
    while b"\r\n\r\n" not in resp:
        chunk = sock.recv(4096)
        if not chunk:
            raise RuntimeError("connection closed during the /ws handshake")
        resp += chunk
    status_line = resp.split(b"\r\n", 1)[0]
    if b"101" not in status_line:
        raise RuntimeError(f"/ws handshake failed: {status_line!r}")


def send_text_frame(sock, payload):
    data = payload.encode()
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
    header = bytearray([0x81])
    length = len(data)
    if length < 126:
        header.append(0x80 | length)
    elif length < 65536:
        header.append(0x80 | 126)
        header += struct.pack(">H", length)
    else:
        header.append(0x80 | 127)
        header += struct.pack(">Q", length)
    header += mask
    sock.sendall(bytes(header) + masked)


def recv_frame(sock):
    b1, b2 = recv_exact(sock, 2)
    opcode = b1 & 0x0F
    masked = (b2 & 0x80) != 0
    length = b2 & 0x7F
    if length == 126:
        length = struct.unpack(">H", recv_exact(sock, 2))[0]
    elif length == 127:
        length = struct.unpack(">Q", recv_exact(sock, 8))[0]
    mask_key = recv_exact(sock, 4) if masked else b""
    payload = recv_exact(sock, length)
    if masked:
        payload = bytes(b ^ mask_key[i % 4] for i, b in enumerate(payload))
    return opcode, payload


sock = socket.create_connection((host, port), timeout=5)
try:
    ws_handshake(sock)
    send_text_frame(sock, json.dumps({"type": "auth", "token": token}))

    deadline = time.time() + 8
    found = False
    while time.time() < deadline:
        sock.settimeout(max(0.1, deadline - time.time()))
        try:
            opcode, payload = recv_frame(sock)
        except socket.timeout:
            break
        if opcode == 0x8:  # close frame
            break
        if opcode != 0x1:  # only text frames carry our envelopes
            continue
        if expected_line in payload.decode(errors="replace"):
            found = True
            break

    if not found:
        print("expected line never arrived over /ws", file=sys.stderr)
        sys.exit(1)
finally:
    sock.close()
PYEOF
then
    cat "$WORKDIR/stdout.log" "$WORKDIR/stderr.log" >&2 || true
    fail "stdin line delivered over /ws" "a text frame containing '$SMOKE_LINE'" "no matching frame received (see log above)"
fi
echo "ok: /ws delivered the piped stdin line"

echo "all smoke checks passed for $BIN"
