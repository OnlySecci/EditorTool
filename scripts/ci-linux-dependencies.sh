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
  archive=https://ports.ubuntu.com/ubuntu-ports
else
  archive=https://archive.ubuntu.com/ubuntu
fi
. /etc/os-release
sudo rm -f /etc/apt/sources.list.d/ubuntu.sources
printf 'deb %s %s main universe restricted multiverse\ndeb %s %s-updates main universe restricted multiverse\ndeb %s %s-security main universe restricted multiverse\n' \
  "$archive" "$VERSION_CODENAME" "$archive" "$VERSION_CODENAME" "$archive" "$VERSION_CODENAME" | sudo tee /etc/apt/sources.list >/dev/null
sudo --preserve-env=http_proxy,https_proxy,no_proxy apt-get -o Acquire::Retries=1 -o Acquire::https::Timeout=20 -o APT::Update::Error-Mode=any update
sudo --preserve-env=http_proxy,https_proxy,no_proxy apt-get -o Acquire::Retries=1 -o Acquire::https::Timeout=20 install -y "$@"
