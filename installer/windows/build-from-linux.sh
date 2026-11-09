#!/usr/bin/env bash
# Builds the Windows installer on Linux (no Windows PC, no Inno Setup, no GitHub Actions needed).
#   sudo apt install mingw-w64 nsis      (once)
#   rustup target add x86_64-pc-windows-gnu   (once)
#   installer/windows/build-from-linux.sh
# Result: dist/vimora-<version>-windows-x64-setup.exe
set -euo pipefail
cd "$(dirname "$0")/../.."
ver=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
# Static C runtime: the programs need only the DLLs every Windows has.
RUSTFLAGS="-C target-feature=+crt-static -C link-arg=-static-libgcc" \
  cargo build --release --target x86_64-pc-windows-gnu -p vimora-server -p vimora-client
mkdir -p dist
makensis -V2 -DVERSION="$ver" \
  -DBINDIR="../../target/x86_64-pc-windows-gnu/release" \
  -DOUTFILE="../../dist/vimora-$ver-windows-x64-setup.exe" \
  installer/windows/vimora.nsi
ls -l "dist/vimora-$ver-windows-x64-setup.exe"
