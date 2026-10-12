#!/usr/bin/env bash
# Packages built release binaries into the tarball a GitHub release carries:
#
#   <out-dir>/prepcook-<version>-<target>.tar.gz
#     prepcook-<version>-<target>/prepcook
#     prepcook-<version>-<target>/prepcook-mcp
#     prepcook-<version>-<target>/LICENSE
#     prepcook-<version>-<target>/NOTICE
#
# <target> is one of linux-x64, macos-x64, macos-arm64 (the names npx-cli
# looks for). Used by .github/workflows/release.yml and local-build.sh so both
# produce the same layout.
#
# usage: scripts/package-release.sh <version> <target> <out-dir>
# CARGO_TARGET_DIR (default: target) is where the release binaries are read.

set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <version> <target> <out-dir>" >&2
  exit 2
fi

version="$1"
target="$2"
out_dir="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"
bin_dir="${CARGO_TARGET_DIR:-$root/target}/release"
name="prepcook-${version}-${target}"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

mkdir -p "$stage/$name" "$out_dir"
cp "$bin_dir/prepcook" "$bin_dir/prepcook-mcp" "$stage/$name/"
cp "$root/LICENSE" "$root/NOTICE" "$stage/$name/"
chmod 755 "$stage/$name/prepcook" "$stage/$name/prepcook-mcp"

tar -czf "$out_dir/$name.tar.gz" -C "$stage" "$name"
echo "$out_dir/$name.tar.gz"
