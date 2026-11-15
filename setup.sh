#!/usr/bin/env bash
# Run by omegga when the plugin is installed: builds the plugin itself.
# The editor is built separately (build-editor.sh), since it needs the wasm
# toolchain and most installs will copy a prebuilt dist/ instead.
cd "$(dirname "$0")" || exit 1
if command -v cargo >/dev/null; then
  cargo build --release -p omegga-arcs
else
  echo "arcs: Rust is not installed, so the plugin could not be built. Install it from https://rustup.rs and run 'cargo build --release -p omegga-arcs' here."
fi
exit 0
