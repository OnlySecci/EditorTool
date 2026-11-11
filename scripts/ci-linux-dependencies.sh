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
  archives=(https://archive.ubuntu.com/ubuntu https://mirrors.edge.kernel.org/ubuntu https://mirror.math.princeton.edu/pub/ubuntu)
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
sudo rm -f /etc/apt/sources.list.d/ubuntu.sources
printf 'deb %s %s main universe restricted multiverse\ndeb %s %s-updates main universe restricted multiverse\ndeb %s %s-security main universe restricted multiverse\n' \
  "$archive" "$VERSION_CODENAME" "$archive" "$VERSION_CODENAME" "$archive" "$VERSION_CODENAME" | sudo tee /etc/apt/sources.list >/dev/null
sudo --preserve-env=http_proxy,https_proxy,no_proxy apt-get -o Acquire::Retries=1 -o Acquire::https::Timeout=20 -o APT::Update::Error-Mode=any update
sudo --preserve-env=http_proxy,https_proxy,no_proxy apt-get -o Acquire::Retries=1 -o Acquire::https::Timeout=20 install -y "$@"
