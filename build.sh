#!/usr/bin/env bash
# Vimora: barcha o'rnatuvchilarni Ubuntu'da yig'ish.
#
#   ./build.sh            hammasi: Ubuntu .deb (o'qituvchi + o'quvchi) va Windows .exe
#   ./build.sh linux      faqat Ubuntu .deb paketlari
#   ./build.sh windows    faqat Windows o'rnatuvchisi
#
# Natija: dist/ papkasida, yonida SHA256SUMS. Hech narsa GitHub'ga yuklanmaydi.
# Yetishmayotgan vositalar birinchi marta o'zi o'rnatiladi (apt uchun parol so'raladi).
#
# Ubuntu paketlari glibc 2.35 uchun yig'iladi (cargo-zigbuild), shuning uchun ular
# Ubuntu 22.04 va undan yangi versiyalarda o'rnatiladi, qaysi kompyuterda yig'ilganidan qat'i nazar.
set -euo pipefail
cd "$(dirname "$0")"

WHAT="${1:-all}"
case "$WHAT" in all | linux | windows) ;; *)
  echo "Ishlatish: ./build.sh [all|linux|windows]"
  exit 2
  ;;
esac

GLIBC="2.35"
VER="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
mkdir -p dist
LOG="dist/build.log"
: >"$LOG"

blue=$'\e[38;2;125;186;40m' red=$'\e[31m' dim=$'\e[2m' off=$'\e[0m'
step() { printf '\n%s▸ %s%s\n' "$blue" "$*" "$off"; }
fail() {
  printf '\n%sXATO:%s %s\n' "$red" "$off" "$*" >&2
  printf '%sTo'\''liq log: %s%s\n' "$dim" "$LOG" "$off" >&2
  exit 1
}
# Run quietly; on failure show the tail of the log.
run() {
  echo "\$ $*" >>"$LOG"
  if ! "$@" >>"$LOG" 2>&1; then
    tail -n 25 "$LOG" >&2
    fail "$1 xato bilan tugadi"
  fi
}

# --- Vositalar -------------------------------------------------------------------------------
apt_need=()
need_apt() { dpkg -s "$1" >/dev/null 2>&1 || apt_need+=("$1"); }

step "Vositalar tekshirilmoqda"
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
command -v cargo >/dev/null || fail "Rust topilmadi. O'rnating: https://rustup.rs  (keyin terminalni qayta oching)"

if [[ $WHAT != windows ]]; then
  need_apt zig
fi
if [[ $WHAT != linux ]]; then
  need_apt mingw-w64
  need_apt nsis
fi
if ((${#apt_need[@]})); then
  echo "O'rnatiladi (apt): ${apt_need[*]}"
  sudo apt-get install -y "${apt_need[@]}" || fail "apt o'rnatishi bajarilmadi"
fi
if [[ $WHAT != windows ]]; then
  cargo zigbuild --help >/dev/null 2>&1 || { echo "O'rnatiladi: cargo-zigbuild"; run cargo install --locked cargo-zigbuild; }
  cargo deb --version >/dev/null 2>&1 || { echo "O'rnatiladi: cargo-deb"; run cargo install --locked cargo-deb; }
fi
if [[ $WHAT != linux ]]; then
  rustup target list --installed | grep -q x86_64-pc-windows-gnu || run rustup target add x86_64-pc-windows-gnu
fi
echo "Tayyor. Versiya: $VER"

# --- Ubuntu ----------------------------------------------------------------------------------
if [[ $WHAT != windows ]]; then
  step "Ubuntu: dastur yig'ilmoqda (glibc $GLIBC uchun)"
  run cargo zigbuild --release --target "x86_64-unknown-linux-gnu.$GLIBC" -p vimora-server -p vimora-client
  step "Ubuntu: .deb paketlar"
  for pkg in vimora-server vimora-client; do
    run cargo deb -p "$pkg" --no-build --target x86_64-unknown-linux-gnu --output dist/
  done
  # Check what we promise: no glibc symbol newer than the target.
  for deb in "dist/vimora-teacher_${VER}_amd64.deb" "dist/vimora-student_${VER}_amd64.deb"; do
    [[ -f $deb ]] || fail "$deb yaratilmadi"
    need=$(dpkg-deb -f "$deb" Depends | grep -o 'libc6 (>= [0-9.]*)' || true)
    echo "  $(basename "$deb")  ${dim}${need}${off}"
  done
fi

# --- Windows ---------------------------------------------------------------------------------
if [[ $WHAT != linux ]]; then
  step "Windows: dastur yig'ilmoqda"
  # Static C runtime: the programs need only the DLLs every Windows has.
  RUSTFLAGS="-C target-feature=+crt-static -C link-arg=-static-libgcc" \
    run cargo build --release --target x86_64-pc-windows-gnu -p vimora-server -p vimora-client
  step "Windows: o'rnatuvchi (NSIS)"
  run makensis -V2 -DVERSION="$VER" \
    -DBINDIR="../../target/x86_64-pc-windows-gnu/release" \
    -DOUTFILE="../../dist/vimora-$VER-windows-x64-setup.exe" \
    installer/windows/vimora.nsi
  [[ -f "dist/vimora-$VER-windows-x64-setup.exe" ]] || fail "Windows o'rnatuvchisi yaratilmadi"
fi

# --- SHA256SUMS ------------------------------------------------------------------------------
step "SHA256SUMS"
(
  cd dist
  files=()
  for f in "vimora-$VER-windows-x64-setup.exe" "vimora-teacher_${VER}_amd64.deb" "vimora-student_${VER}_amd64.deb"; do
    [[ -f $f ]] && files+=("$f")
  done
  sha256sum "${files[@]}" >SHA256SUMS
)

printf '\n%sHAMMASI TAYYOR%s  (dist/)\n' "$blue" "$off"
(cd dist && ls -lh --time-style=+ SHA256SUMS vimora-*"$VER"* 2>/dev/null | awk '{printf "  %-45s %s\n", $NF, $5}')
