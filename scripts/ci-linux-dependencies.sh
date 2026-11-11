#!/usr/bin/env bash
set -euo pipefail
# Hosted runners behind an HTTPS proxy expose uppercase variables; APT uses
# lowercase names, and sudo normally strips them. Do not print proxy values.
export https_proxy="${https_proxy:-${HTTPS_PROXY:-}}"
export http_proxy="${http_proxy:-${HTTP_PROXY:-}}"
export no_proxy="${no_proxy:-${NO_PROXY:-}}"
# Use canonical HTTPS archives instead of the runner image's Azure HTTP mirror.
# Keep signature verification and fail quickly if the build network is down.
arch=$(dpkg --print-architecture)
if [ "$arch" = arm64 ]; then
  archives=(https://ports.ubuntu.com/ubuntu-ports https://mirrors.mit.edu/ubuntu-ports)
else
  archives=(https://archive.ubuntu.com/ubuntu https://mirror.math.princeton.edu/pub/ubuntu https://mirrors.edge.kernel.org/ubuntu)
fi
. /etc/os-release
archive=
for candidate in "${archives[@]}"; do
  if curl --fail --silent --show-error --location --connect-timeout 5 --max-time 12 --output /dev/null "$candidate/dists/$VERSION_CODENAME/InRelease"; then
    archive=$candidate
    break
  fi
done
if [ -z "$archive" ]; then
  echo 'No signed Ubuntu package archive reachable from this runner.' >&2
  exit 1
fi
printf 'Build package archive: %s\n' "$archive"
# Keep update and install on the exact same source, architecture and package
# lists. Runner image caches and unrelated repositories must not affect them.
apt_dir="${RUNNER_TEMP:-/tmp}/editortool-apt"
sudo install -d -m 755 "$apt_dir" "$apt_dir/lists"
sudo install -d -m 700 -o _apt "$apt_dir/lists/partial"
for suite in "$VERSION_CODENAME" "$VERSION_CODENAME-updates" "$VERSION_CODENAME-security"; do
  printf 'deb [arch=%s signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] %s %s main universe restricted multiverse\n' "$arch" "$archive" "$suite"
done | sudo tee "$apt_dir/sources.list" >/dev/null
apt_options=(
  -o "Dir::Etc::sourcelist=$apt_dir/sources.list"
  -o 'Dir::Etc::sourceparts=-'
  -o "Dir::State::lists=$apt_dir/lists"
  -o "Dir::Cache::pkgcache=$apt_dir/pkgcache.bin"
  -o "Dir::Cache::srcpkgcache=$apt_dir/srcpkgcache.bin"
  -o "APT::Architecture=$arch"
  -o Acquire::Retries=1
  -o Acquire::https::Timeout=20
  -o APT::Update::Error-Mode=any
)
sudo --preserve-env=http_proxy,https_proxy,no_proxy apt-get "${apt_options[@]}" update
sudo apt-cache "${apt_options[@]}" policy "$@"
sudo --preserve-env=http_proxy,https_proxy,no_proxy apt-get "${apt_options[@]}" install -y "$@"
