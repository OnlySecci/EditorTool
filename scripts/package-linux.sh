#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
debarch=$(dpkg --print-architecture)
formats=",${PACKAGE_FORMATS:-portable,rpm,deb},"
IFS=, read -ra selected_formats <<< "${PACKAGE_FORMATS:-portable,rpm,deb}"
for format in "${selected_formats[@]}"; do
  case "$format" in portable|rpm|deb) ;; *) echo "Unknown package format: $format" >&2; exit 1 ;; esac
done
wants() { [[ "$formats" == *",$1,"* ]]; }
pkgroot="$PWD/dist/pkgroot"
mkdir -p "$pkgroot/usr/bin" "$pkgroot/usr/share/applications" "$pkgroot/usr/share/icons/hicolor/256x256/apps" "$pkgroot/usr/share/mime/packages" "$pkgroot/usr/share/doc/editortool" dist/packages
install -m755 dist/linux/EditorTool "$pkgroot/usr/bin/editortool"
cp packaging/editortool.desktop "$pkgroot/usr/share/applications/com.github.OnlySecci.EditorTool.desktop"
cp assets/editortool.png "$pkgroot/usr/share/icons/hicolor/256x256/apps/editortool.png"
cp packaging/editortool-mime.xml "$pkgroot/usr/share/mime/packages/"
cp README.md LICENSE "$pkgroot/usr/share/doc/editortool/"
mkdir -p "$pkgroot/usr/share/doc/editortool/licenses"
cp -R third-party-licenses/. "$pkgroot/usr/share/doc/editortool/licenses/"
find "$pkgroot" -type d -exec chmod 755 '{}' +
find "$pkgroot/usr/share" -type f -exec chmod 644 '{}' +
if wants portable; then
  tar --mode=u+rwX,go+rX -czf "dist/packages/EditorTool-$version-linux-$arch-portable.tar.gz" -C dist/linux .
fi
if wants rpm; then
rm -rf "$pkgroot/DEBIAN"
topdir="$PWD/dist/rpmbuild"
mkdir -p "$topdir/BUILD" "$topdir/BUILDROOT" "$topdir/RPMS" "$topdir/SOURCES" "$topdir/SPECS" "$topdir/SRPMS"
cat > "$topdir/SPECS/editortool.spec" <<EOF
Name: editortool
Version: $version
Release: 1
Summary: Lightweight text and image editor
License: Unlicense
URL: https://github.com/OnlySecci/EditorTool
BuildArch: $arch
Requires: /usr/bin/pkexec
%global debug_package %{nil}
%global _build_id_links none
%description
Native Rust/FLTK editor with text, images, syntax colors and session recovery.
%install
mkdir -p %{buildroot}
cp -a $pkgroot/. %{buildroot}/
%files
/usr/bin/editortool
/usr/share/applications/com.github.OnlySecci.EditorTool.desktop
/usr/share/icons/hicolor/256x256/apps/editortool.png
/usr/share/mime/packages/editortool-mime.xml
/usr/share/doc/editortool
EOF
rpmbuild --define "_topdir $topdir" -bb "$topdir/SPECS/editortool.spec"
find "$topdir/RPMS" -name '*.rpm' -exec cp '{}' dist/packages/ \;
fi
if wants deb; then
mkdir -p "$pkgroot/DEBIAN"
cat > "$pkgroot/DEBIAN/control" <<EOF
Package: editortool
Version: $version
Section: editors
Priority: optional
Architecture: $debarch
Maintainer: OnlySecci <192268366+OnlySecci@users.noreply.github.com>
Depends: pkexec, libc6 (>= 2.39), libstdc++6, libx11-6, libxext6, libxft2, libxinerama1, libxcursor1, libxrender1, libxfixes3, libpango-1.0-0, libpangocairo-1.0-0, libpangoxft-1.0-0, libcairo2, libfontconfig1
Description: Lightweight native text and image editor
 Session recovery, syntax highlighting, plain-text clipboard and embedded images.
EOF
find "$pkgroot" -type d -exec chmod 755 '{}' +
find "$pkgroot/usr/share" -type f -exec chmod 644 '{}' +
chmod 644 "$pkgroot/DEBIAN/control"
dpkg-deb --root-owner-group --build "$pkgroot" "dist/packages/editortool_${version}_${debarch}.deb"

fi
