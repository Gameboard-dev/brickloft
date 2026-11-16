#!/usr/bin/env bash
# Builds the editor as a page for a browser, into dist/. An optional extra:
# the page shapes and measures forms as the program does, but a browser page
# cannot put a file on the clipboard, so it has no way to hand a form to the
# game. Serve dist/ with any local web server and open index.html.
#
# Needs the wasm target and the wasm-bindgen command, at the version pinned
# in editor/Cargo.toml (the two must match exactly):
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.129
set -euo pipefail
cd "$(dirname "$0")"
command -v wasm-bindgen >/dev/null || { echo "build-editor: wasm-bindgen is not installed; see the top of this file." >&2; exit 1; }
cargo build -p brickloft-editor --release --target wasm32-unknown-unknown
mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name brickloft_editor \
  target/wasm32-unknown-unknown/release/brickloft_editor.wasm
cp editor/web/index.html editor/web/style.css editor/web/loader.js editor/web/icon.svg dist/
ls -l dist
