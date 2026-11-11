#!/bin/sh
# Vimora standalone updater (student computers, Ubuntu).
#
# Run by vimora-updater.timer (5 minutes after start-up and every 6 hours), as root. It does
# not depend on the Vimora program: even if a Vimora version could not update itself, this
# script still brings it to the newest release.
#
# Newest release (the releases/latest redirect) -> compare with the installed package ->
# download .deb + SHA256SUMS -> verify -> apt install. Respects the teacher's "automatic update
# off" and leaves a frozen (overlayroot) PC to Vimora's own commit cycle.
# Log: /var/lib/vimora/updates/updater.log

REPO=veymay/vimora
DIR=/var/lib/vimora/updates
LOG=$DIR/updater.log
mkdir -p "$DIR"

log() {
    echo "$(date '+%F %T')  $*" >> "$LOG"
    tail -n 200 "$LOG" > "$LOG.tmp" 2>/dev/null && mv "$LOG.tmp" "$LOG"
}

fetch() { # url file
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --max-time 300 -o "$2" "$1"
    else
        wget -q -T 60 -O "$2" "$1"
    fi
}

latest_tag() {
    url="https://github.com/$REPO/releases/latest"
    if command -v curl >/dev/null 2>&1; then
        loc=$(curl -fsS -o /dev/null --max-time 30 -w '%{redirect_url}' "$url")
    else
        loc=$(wget --max-redirect=0 -S -T 30 -O /dev/null "$url" 2>&1 | awk '/^ *Location:/ {print $2}' | tail -n 1)
    fi
    echo "${loc##*/}" | tr -d '\r'
}

# The teacher turned automatic updates off.
if grep -q '"vimora_manual":true' /var/lib/vimora/shield.json 2>/dev/null; then
    exit 0
fi
# Frozen (overlayroot): Vimora installs updates in its own commit cycle.
if grep -q ' / overlay ' /proc/mounts 2>/dev/null; then
    exit 0
fi

installed=$(dpkg-query -W -f='${Version}' vimora-student 2>/dev/null)
[ -n "$installed" ] || installed=0.0.0
tag=$(latest_tag)
latest=${tag#v}
[ -n "$latest" ] || exit 0
dpkg --compare-versions "$latest" gt "$installed" || exit 0

log "installed $installed, newest $latest: updating"
name="vimora-student_${latest}_amd64.deb"
base="https://github.com/$REPO/releases/download/$tag"
file="$DIR/updater-$name"
if ! fetch "$base/SHA256SUMS" "$DIR/SHA256SUMS.updater"; then log "no SHA256SUMS"; exit 1; fi
if ! fetch "$base/$name" "$file"; then log "download failed"; rm -f "$file"; exit 1; fi
want=$(awk -v n="$name" '$2 == n || $2 == "*" n {print $1}' "$DIR/SHA256SUMS.updater")
got=$(sha256sum "$file" | cut -d' ' -f1)
if [ -z "$want" ] || [ "$want" != "$got" ]; then
    log "downloaded file is damaged (SHA-256 mismatch)"
    rm -f "$file"
    exit 1
fi
if DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=600 install -y --reinstall "$file" >> "$LOG" 2>&1; then
    log "installed $latest"
else
    log "apt-get failed"
fi
rm -f "$file"
