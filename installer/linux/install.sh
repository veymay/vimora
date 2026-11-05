#!/usr/bin/env bash
# Vimora installer for Ubuntu / Debian.
#
#   sudo ./install.sh                         # interactive: asks teacher or student
#   sudo ./install.sh --role teacher
#   sudo ./install.sh --role student --key "maxfiy" [--server 192.168.1.10:47801] [--name PC-12] [--x11]
#
# Looks for vimora-server / vimora-client next to this script, else in ../../target/release.
set -euo pipefail

ROLE="" ROOM="" KEY="" SERVER="" NAME="" FORCE_X11=""
PREFIX=/opt/vimora
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

c_blue=$'\e[38;2;45;129;255m'; c_dim=$'\e[2m'; c_bold=$'\e[1m'; c_red=$'\e[31m'; c_off=$'\e[0m'
say()  { printf '%s▸%s %s\n' "$c_blue" "$c_off" "$*"; }
die()  { printf '%sXato:%s %s\n' "$c_red" "$c_off" "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role) ROLE="$2"; shift 2 ;;
    --room) ROOM="$2"; shift 2 ;;
    --key) KEY="$2"; shift 2 ;;
    --server) SERVER="$2"; shift 2 ;;
    --name) NAME="$2"; shift 2 ;;
    --x11) FORCE_X11=1; shift ;;
    --prefix) PREFIX="$2"; shift 2 ;;
    -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
    *) die "noma'lum parametr: $1" ;;
  esac
done

[[ $EUID -eq 0 ]] || die "root kerak: sudo $0 $*"

find_bin() {
  for d in "$HERE" "$HERE/bin" "$HERE/../../target/release"; do
    [[ -x "$d/$1" ]] && { echo "$d/$1"; return; }
  done
  die "$1 topilmadi (avval: cargo build --release)"
}

printf '\n%s  VIMORA%s  %so'"'"'rnatuvchi%s\n\n' "$c_bold" "$c_off" "$c_dim" "$c_off"

if [[ -z "$ROLE" ]]; then
  echo "  Bu kompyuter kimniki?"
  echo "    1) O'qituvchi  — nazorat konsoli (vimora-server)"
  echo "    2) O'quvchi    — agent (vimora-client)"
  read -rp "  Tanlang [1/2]: " ans
  case "$ans" in 1) ROLE=teacher ;; 2) ROLE=student ;; *) die "1 yoki 2 ni tanlang" ;; esac
fi
[[ "$ROLE" == teacher || "$ROLE" == student ]] || die "--role teacher|student"

install -d "$PREFIX/bin" /usr/share/icons/hicolor/scalable/apps
install -m 644 "$HERE/vimora.svg" /usr/share/icons/hicolor/scalable/apps/vimora.svg 2>/dev/null || true

ufw_allow() { command -v ufw >/dev/null && ufw status | grep -q "Status: active" && ufw allow "$1" >/dev/null && say "ufw: $1 ochildi" || true; }

# ------------------------------------------------------------------------------------------------
if [[ "$ROLE" == teacher ]]; then
  install -m 755 "$(find_bin vimora-server)" "$PREFIX/bin/vimora-server"
  ln -sf "$PREFIX/bin/vimora-server" /usr/local/bin/vimora-server
  cat > /usr/share/applications/vimora-server.desktop <<EOF
[Desktop Entry]
Type=Application
Name=Vimora
GenericName=O'qituvchi konsoli
Comment=Kompyuter xonalarini nazorat qilish
Exec=$PREFIX/bin/vimora-server
Icon=vimora
Terminal=false
Categories=Education;Network;
EOF
  ufw_allow 47801/tcp
  say "O'qituvchi konsoli o'rnatildi. Ilovalar menyusidan \"Vimora\" ni oching."
  exit 0
fi

# ------------------------------------------------------------------------------------------------
# Student
if [[ -z "$KEY" ]]; then read -rsp "  Ulanish kaliti (o'qituvchi konsolidagi bilan bir xil): " KEY; echo; fi
[[ -n "$KEY" ]] || die "ulanish kaliti bo'sh bo'lmasligi kerak"
if [[ -z "$SERVER" && -t 0 ]]; then read -rp "  Konsol manzili (ixtiyoriy, masalan 192.168.1.10:47801): " SERVER; fi

install -m 755 "$(find_bin vimora-client)" "$PREFIX/bin/vimora-client"
ln -sf "$PREFIX/bin/vimora-client" /usr/local/bin/vimora-client

args=(--key "$KEY")
[[ -n "$SERVER" ]] && args+=(--server "$SERVER")
[[ -n "$NAME" ]] && args+=(--name "$NAME")
# Writes /etc/vimora/client.json (no key) and /etc/vimora/service.json (key, root only),
# installs and starts vimora-service.service (root: freeze, updates, NetControl).
"$PREFIX/bin/vimora-client" "${args[@]}" setup
chmod 644 /etc/vimora/client.json
say "Vimora xizmati ishga tushdi (systemctl status vimora-service)"

cat > /etc/xdg/autostart/vimora-client.desktop <<EOF
[Desktop Entry]
Type=Application
Name=Vimora Agent
Exec=$PREFIX/bin/vimora-client
NoDisplay=true
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Phase=Applications
EOF
say "Avtoyuklash: har bir foydalanuvchi kirganda agent ishga tushadi"
ufw_allow 47800/udp

# Screen capture, lock-screen input blocking and the keyword watcher need an X11 session.
if [[ -f /etc/gdm3/custom.conf ]]; then
  if [[ -z "$FORCE_X11" && -t 0 ]]; then
    read -rp "  Wayland o'chirilib, X11 seansi yoqilsinmi? (ekranni ko'rish uchun kerak) [Y/n]: " a
    [[ "${a:-Y}" =~ ^[Yy]$ ]] && FORCE_X11=1
  fi
  if [[ -n "$FORCE_X11" ]]; then
    if grep -q '^#\?WaylandEnable' /etc/gdm3/custom.conf; then
      sed -i 's/^#\?WaylandEnable=.*/WaylandEnable=false/' /etc/gdm3/custom.conf
    else
      sed -i '/^\[daemon\]/a WaylandEnable=false' /etc/gdm3/custom.conf
    fi
    say "GDM: X11 seansi yoqildi"
  fi
fi

say "O'quvchi agenti o'rnatildi. Kompyuterni qayta yoqing (guruh va seans o'zgarishlari uchun)."
