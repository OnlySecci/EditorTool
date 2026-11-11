#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
if [[ "${CARGO_BUILD_TARGET:-}" == aarch64-apple-darwin ]]; then
  export MACOSX_DEPLOYMENT_TARGET=11.0
fi
cargo build --release --locked
bundle="dist/macos/EditorTool.app/Contents"
mkdir -p "$bundle/MacOS" "$bundle/Resources"
cp assets/editortool.icns "$bundle/Resources/EditorTool.icns"
cp README.md LICENSE dist/macos/
cp LICENSE "$bundle/Resources/"
cp -R third-party-licenses "$bundle/Resources/"
cp "target/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release/editortool" "$bundle/MacOS/EditorTool"
chmod +x "$bundle/MacOS/EditorTool"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
cat > "$bundle/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>EditorTool</string>
<key>CFBundleIdentifier</key><string>com.github.OnlySecci.EditorTool</string>
<key>CFBundleIconFile</key><string>EditorTool.icns</string>
<key>CFBundleName</key><string>EditorTool</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleDocumentTypes</key><array><dict><key>CFBundleTypeName</key><string>EditorTool Document</string><key>CFBundleTypeRole</key><string>Editor</string><key>CFBundleTypeExtensions</key><array><string>etdoc</string></array></dict></array>
<key>NSHighResolutionCapable</key><true/>
<key>NSLocalNetworkUsageDescription</key><string>EditorTool sends and synchronizes documents with devices you approve on internal networks.</string>
<key>NSHumanReadableCopyright</key><string>EditorTool contributors</string>
</dict></plist>
PLIST
printf 'Erstellt: %s/dist/macos/EditorTool.app\n' "$PWD"
