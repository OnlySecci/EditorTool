#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
case "${CARGO_BUILD_TARGET:-}" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x86_64 ;;
  *) arch=$(uname -m) ;;
esac
# Verify the compiled Mach-O target without executing the application.
lipo dist/macos/EditorTool.app/Contents/MacOS/EditorTool -verify_arch "$arch"
mkdir -p dist/packages dist/dmg-root
codesign --force --deep --sign - dist/macos/EditorTool.app
cp -R dist/macos/EditorTool.app dist/dmg-root/
cp README.md LICENSE dist/dmg-root/
ln -s /Applications dist/dmg-root/Applications
hdiutil create -volname EditorTool -srcfolder dist/dmg-root -ov -format UDZO "dist/packages/EditorTool-$version-macos-$arch.dmg"
ditto -c -k --keepParent dist/macos/EditorTool.app "dist/packages/EditorTool-$version-macos-$arch.zip"
