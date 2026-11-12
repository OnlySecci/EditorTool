#!/usr/bin/env bash
# Source this file before a bundled-FLTK build. Never use upstream curl -k.
set -euo pipefail
if [[ -n "${CFLTK_BUNDLE_DIR:-}" ]]; then return 0; fi
fltk_target=${CARGO_BUILD_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
case "$fltk_target" in
  x86_64-unknown-linux-gnu) fltk_sha=dcd3032f287bc75b5aede3a08e44845cf140a0c943b6aa3b4ef2ddcf98857226 ;;
  aarch64-unknown-linux-gnu) fltk_sha=bd62d4eb06824fb04a559724da523bf2c1a1b88dfe5d07ead999bc783023dcf6 ;;
  x86_64-apple-darwin) fltk_sha=055612bb2aa89de1145d55a77ffb869d128aae3fa0e0ec7868d83de55832b437 ;;
  aarch64-apple-darwin) fltk_sha=4c3669ac2908318f96810063dfa2f01d96b4653ad40aaf1bf4c9772534da001a ;;
  *) echo "Unsupported bundled FLTK target: $fltk_target" >&2; return 1 ;;
esac
export CFLTK_BUNDLE_DIR="$PWD/target/fltk-bundle/$fltk_target/lib"
fltk_archive="$PWD/target/fltk-bundle/$fltk_target/bundle.tar.gz"
mkdir -p "$(dirname "$fltk_archive")"
if [[ ! -f "$fltk_archive" ]]; then
  curl --fail --show-error --silent --location "https://github.com/MoAlyousef/cfltk/releases/download/1.5.23/lib_$fltk_target.tar.gz" -o "$fltk_archive.pending"
  mv "$fltk_archive.pending" "$fltk_archive"
fi
if command -v sha256sum >/dev/null; then
  printf '%s  %s\n' "$fltk_sha" "$fltk_archive" | sha256sum -c -
else
  printf '%s  %s\n' "$fltk_sha" "$fltk_archive" | shasum -a 256 -c -
fi
tar -xzf "$fltk_archive" -C "$(dirname "$fltk_archive")"
