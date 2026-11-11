#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user --noninteractive -y flathub "org.freedesktop.Platform/$arch/25.08" "org.freedesktop.Sdk/$arch/25.08"
flatpak-builder --user --disable-rofiles-fuse --force-clean --repo=dist/flatpak-repo dist/flatpak-build packaging/flatpak.yml
flatpak build-bundle --arch="$arch" dist/flatpak-repo "dist/packages/EditorTool-$version-linux-$arch.flatpak" com.github.OnlySecci.EditorTool --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
