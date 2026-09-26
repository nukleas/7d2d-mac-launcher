#!/usr/bin/env bash
#
# Provision a 7 Days to Die + Undead Legacy dedicated server (Debian/Ubuntu, x86_64).
#
#   sudo ./setup-ul-server.sh
#
# Re-running is safe: it updates the game, re-applies the pinned mod build and
# restarts the service.
#
# The mod build is PINNED. 7 Days to Die refuses connections between mismatched
# client and server mod versions, and Undead Legacy 2.7 publishes builds every
# few days — `main` moved during a single 40-minute download while this was
# being written. These SHAs must match the ones in src-tauri/src/channel.rs so
# every player's launcher installs exactly what the server runs.
set -euo pipefail

# Fallbacks, used only if the published build list can't be read. Clients read
# that same list, so editing it in one place keeps server and players in step.
UL_PART1_SHA="bf085dc55fa8b51853b47ec1a88ec4532b6301d5"
UL_PART2_SHA="39e80938f70ba253aedb2c71c70d6ecf6886a7b3"

PIN_MANIFEST_URL="https://raw.githubusercontent.com/nukleas/7d2d-mac-launcher/main/pinned-build.json"

SERVER_USER="sdtd"
SERVER_DIR="/srv/7dtd"
STEAM_DIR="/opt/steamcmd"
STEAM_APP_ID="294420"

log() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
die() { printf '\033[1;31merror: %s\033[0m\n' "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "run as root (sudo $0)"
[[ "$(uname -m)" == "x86_64" ]] || die "7DTD dedicated server is x86_64 only; this box is $(uname -m)"

log "Resolving which Undead Legacy build to install"
# Read the same published list the launchers use. If it can't be fetched or
# parsed we keep the fallbacks above — a server that installs a slightly older
# pinned build is recoverable; one that installs an unknown build is not.
if MANIFEST="$(curl -fsSL --max-time 20 "$PIN_MANIFEST_URL" 2>/dev/null)"; then
  RESOLVED="$(printf '%s' "$MANIFEST" | python3 -c '
import json, re, sys
try:
    parts = json.load(sys.stdin)["channels"]["experimental"]["parts"]
    shas = [p["sha"] for p in parts]
except Exception:
    sys.exit(1)
if len(shas) != 2 or not all(re.fullmatch(r"[0-9a-f]{40}", s) for s in shas):
    sys.exit(1)
print(" ".join(shas))
' 2>/dev/null)" && [[ -n "$RESOLVED" ]] && read -r UL_PART1_SHA UL_PART2_SHA <<<"$RESOLVED" \
    && echo "  using published build list" \
    || echo "  published list unusable — keeping built-in pins"
else
  echo "  could not fetch published list — keeping built-in pins"
fi
echo "  part1 ${UL_PART1_SHA:0:8}   part2 ${UL_PART2_SHA:0:8}"

log "Installing dependencies"
dpkg --add-architecture i386
apt-get update -qq
# steamcmd is 32-bit; the server itself needs 64-bit runtime libs.
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
  curl unzip tar ca-certificates lib32gcc-s1 lib32stdc++6 libsdl2-2.0-0 xmlstarlet

log "Creating service user '$SERVER_USER'"
if ! id -u "$SERVER_USER" >/dev/null 2>&1; then
  # No login shell: this account only ever runs the game server.
  useradd --system --create-home --home-dir "/home/$SERVER_USER" --shell /usr/sbin/nologin "$SERVER_USER"
fi
mkdir -p "$SERVER_DIR" "$STEAM_DIR"
chown -R "$SERVER_USER:$SERVER_USER" "$SERVER_DIR" "$STEAM_DIR"

log "Installing SteamCMD"
if [[ ! -x "$STEAM_DIR/steamcmd.sh" ]]; then
  curl -fsSL "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz" \
    | tar -xz -C "$STEAM_DIR"
  chown -R "$SERVER_USER:$SERVER_USER" "$STEAM_DIR"
fi

log "Installing / updating 7 Days to Die dedicated server (app $STEAM_APP_ID)"
sudo -u "$SERVER_USER" "$STEAM_DIR/steamcmd.sh" \
  +force_install_dir "$SERVER_DIR" \
  +login anonymous \
  +app_update "$STEAM_APP_ID" validate \
  +quit

[[ -f "$SERVER_DIR/7DaysToDieServer.x86_64" ]] || die "server binary missing after steamcmd"

log "Installing Undead Legacy (pinned build)"
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

fetch_part() {
  local part="$1" sha="$2"
  local repo="UndeadLegacyExperimentalPart${part}"
  local url="https://gitlab.com/subquakesgroup/${repo}/-/archive/${sha}/${repo}-${sha}.zip"
  local zip="$STAGING/part${part}.zip"

  # GitLab ignores Range headers, so an interrupted transfer restarts from
  # zero — part 1 is ~3.3 GB. Retry generously rather than fail the run.
  log "  downloading part $part (${sha:0:8})"
  curl -fL --retry 10 --retry-all-errors --retry-delay 5 --max-time 7200 -o "$zip" "$url" \
    || die "download of part $part failed"

  # Part 2 trips unzip's zip-bomb heuristic ("invalid zip file with overlapped
  # components") and is refused outright, despite being a valid archive —
  # verified with the check disabled and with Python's zipfile. Without this the
  # script fails on a perfectly good download.
  UNZIP_DISABLE_ZIPBOMB_DETECTION=TRUE unzip -q "$zip" -d "$STAGING/part${part}"
  # Archives wrap everything in "<repo>-<sha>/". Merge the *contents* of that
  # wrapper: both parts populate Mods/UndeadLegacy/Resources, so replacing the
  # directory instead of merging would silently delete the other part's files.
  cp -a "$STAGING/part${part}/${repo}-${sha}/." "$SERVER_DIR/"
  rm -rf "$STAGING/part${part}" "$zip"
}

fetch_part 1 "$UL_PART1_SHA"
fetch_part 2 "$UL_PART2_SHA"

# Verify both parts actually landed, using files unique to each.
[[ -f "$SERVER_DIR/Mods/UndeadLegacy/Resources/Subquake_Core.ulm"  ]] || die "part 1 payload missing"
[[ -f "$SERVER_DIR/Mods/UndeadLegacy/Resources/Subquake_Items.ulm" ]] || die "part 2 payload missing"
[[ -f "$SERVER_DIR/doorstop_libs/libdoorstop_x64.so" ]] || die "Doorstop loader missing"
chmod +x "$SERVER_DIR/run_bepinex.sh" "$SERVER_DIR/run_bepinex_server.sh"

log "Configuring server"
CONFIG="$SERVER_DIR/serverconfig.xml"
[[ -f "$CONFIG" ]] || die "serverconfig.xml missing"
cp -n "$CONFIG" "$CONFIG.orig" || true

set_prop() {
  xmlstarlet ed -L -u "/ServerSettings/property[@name='$1']/@value" -v "$2" "$CONFIG" 2>/dev/null \
    || printf 'warning: could not set %s\n' "$1" >&2
}

# EasyAntiCheat must be off: it rejects the modded client, so a modded server
# with EAC on is unjoinable no matter what the players do.
set_prop EACEnabled false
set_prop ServerPort 26900

chown -R "$SERVER_USER:$SERVER_USER" "$SERVER_DIR"

log "Installing systemd service"
cat > /etc/systemd/system/7dtd.service <<UNIT
[Unit]
Description=7 Days to Die dedicated server (Undead Legacy)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$SERVER_USER
WorkingDirectory=$SERVER_DIR
# UL's launcher wraps run_bepinex.sh, which injects Doorstop and already
# passes -noeac -dedicated -batchmode.
ExecStart=$SERVER_DIR/run_bepinex_server.sh -configfile=serverconfig.xml
Restart=on-failure
RestartSec=15
# A blood moon on a modded world is the memory spike that kills undersized boxes.
OOMPolicy=continue

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable 7dtd.service
systemctl restart 7dtd.service

if command -v ufw >/dev/null 2>&1; then
  log "Opening firewall ports"
  ufw allow 26900/tcp >/dev/null
  ufw allow 26900:26902/udp >/dev/null
fi

log "Done"
cat <<EOF

  Server dir : $SERVER_DIR
  Service    : systemctl status 7dtd
  Logs       : journalctl -u 7dtd -f
  Mod build  : part1 ${UL_PART1_SHA:0:8}  part2 ${UL_PART2_SHA:0:8}

  Open these on the provider firewall as well as the host:
    26900/tcp, 26900-26902/udp

  Players must run the SAME pinned Undead Legacy build, or the server will
  refuse their connection. The launcher pins these same SHAs.

EOF
