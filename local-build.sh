#!/bin/bash
# Builds the web app and the release binaries for this machine and packages
# them the way a GitHub release does (scripts/package-release.sh), into
# npx-cli/dist/, where the npx CLI picks them up instead of downloading.

set -euo pipefail

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64) ARCH="x64" ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

case "$OS" in
  linux) OS="linux" ;;
  darwin) OS="macos" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

TARGET="${OS}-${ARCH}"
VERSION=$(node -p "require('./npx-cli/package.json').version")

echo "Platform: $TARGET, version: $VERSION"

rm -rf npx-cli/dist
mkdir -p npx-cli/dist

echo "Building the web app..."
pnpm -C packages/local-web build

echo "Building the Rust binaries..."
cargo build --release --bin prepcook --bin prepcook-mcp

echo "Packaging..."
bash scripts/package-release.sh "$VERSION" "$TARGET" npx-cli/dist

echo "Building the npx CLI..."
(cd npx-cli && npm ci && npm run build)

echo ""
echo "To test locally: cd npx-cli && node bin/cli.js"
