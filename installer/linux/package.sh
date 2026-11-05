#!/usr/bin/env bash
# Builds release binaries and bundles them with the installer:
#   dist/vimora-<version>-linux-x86_64.tar.gz  →  tar xf …; cd vimora-…; sudo ./install.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
NAME="vimora-$VERSION-linux-x86_64"

cargo build --release -p vimora-server -p vimora-client
rm -rf "dist/$NAME" && mkdir -p "dist/$NAME"
cp target/release/vimora-server target/release/vimora-client "dist/$NAME/"
cp installer/linux/install.sh installer/linux/uninstall.sh installer/linux/vimora.svg "dist/$NAME/"
chmod +x "dist/$NAME/"*.sh
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "dist/$NAME.tar.gz"
