#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
mkdir -p dist/packages dist/dmg-root
codesign --force --deep --sign - dist/macos/EditorTool.app
cp -R dist/macos/EditorTool.app dist/dmg-root/
cp README.md LICENSE dist/dmg-root/
ln -s /Applications dist/dmg-root/Applications
hdiutil create -volname EditorTool -srcfolder dist/dmg-root -ov -format UDZO "dist/packages/EditorTool-$version-macos-$arch.dmg"
ditto -c -k --keepParent dist/macos/EditorTool.app "dist/packages/EditorTool-$version-macos-$arch.zip"
