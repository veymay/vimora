#!/usr/bin/env bash
# Removes Vimora (teacher console and/or student agent) from Ubuntu / Debian.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "root kerak: sudo $0" >&2; exit 1; }
PREFIX=/opt/vimora

systemctl disable --now vimora-service 2>/dev/null || true
rm -f /etc/systemd/system/vimora-service.service && systemctl daemon-reload 2>/dev/null || true
pkill -f "$PREFIX/bin/vimora-client" 2>/dev/null || true
rm -f /usr/local/bin/vimora-server /usr/local/bin/vimora-client \
      /usr/share/applications/vimora-server.desktop /etc/xdg/autostart/vimora-client.desktop \
      /usr/share/icons/hicolor/scalable/apps/vimora.svg
rm -rf "$PREFIX" /etc/vimora

# Remove our hosts block and give /etc/hosts back to root.
if [[ -f /etc/hosts ]]; then
  sed -i '/^# >>> vimora netcontrol/,/^# <<< vimora netcontrol/d' /etc/hosts
  chown root:root /etc/hosts && chmod 644 /etc/hosts
fi
for d in /etc/opt/chrome /etc/chromium /etc/opt/edge /etc/brave /etc/opt/yandex/browser; do
  rm -f "$d/policies/managed/vimora.json"
done
echo "Vimora o'chirildi."
