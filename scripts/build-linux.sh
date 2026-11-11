#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked
mkdir -p dist/linux
cp "target/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release/editortool" dist/linux/EditorTool
chmod +x dist/linux/EditorTool
cp README.md LICENSE dist/linux/
printf 'Erstellt: %s/dist/linux/EditorTool\n' "$PWD"
