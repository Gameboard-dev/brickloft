#!/usr/bin/env bash
# Builds the editor for the browser into dist/, which the plugin serves.
#
# Needs the wasm target and the wasm-bindgen command, at the version pinned
# in editor/Cargo.toml (the two must match exactly):
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.129
set -euo pipefail
cd "$(dirname "$0")"
command -v wasm-bindgen >/dev/null || { echo "build-editor: wasm-bindgen is not installed; see the top of this file." >&2; exit 1; }
cargo build -p arcs-editor --release --target wasm32-unknown-unknown
mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name arcs_editor \
  target/wasm32-unknown-unknown/release/arcs_editor.wasm
cp editor/web/index.html editor/web/style.css editor/web/loader.js editor/web/icon.svg dist/
ls -l dist
