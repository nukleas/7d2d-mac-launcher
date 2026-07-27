#!/usr/bin/env bash
# Build a friend-ready zip: app + Open Me First (Sequoia quarantine fix) + guide.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

APP_SRC="$ROOT/src-tauri/target/release/bundle/macos/7D2D Mac Launcher.app"
OUT_DIR="$ROOT/dist-release"
PACK="$OUT_DIR/UndeadLegacy-Mac-Setup"
ZIP="$OUT_DIR/UndeadLegacy-Mac-Setup.zip"

echo "==> Building release app/dmg…"
bun run package

if [[ ! -d "$APP_SRC" ]]; then
  echo "error: expected app at $APP_SRC" >&2
  exit 1
fi

rm -rf "$PACK" "$ZIP"
mkdir -p "$PACK"

echo "==> Staging app…"
ditto "$APP_SRC" "$PACK/7D2D Mac Launcher.app"
codesign --force --deep --sign - "$PACK/7D2D Mac Launcher.app"
xattr -cr "$PACK/7D2D Mac Launcher.app" 2>/dev/null || true

echo "==> Building Open Me First helper…"
TMP_AS=$(mktemp /tmp/open_me_first.XXXXXX.applescript)
cat > "$TMP_AS" <<'APPLESCRIPT'
set mePosix to POSIX path of (path to me as text)
set packFolder to do shell script "dirname " & quoted form of mePosix
set launcher to packFolder & "/7D2D Mac Launcher.app"
set dest to (POSIX path of (path to applications folder)) & "7D2D Mac Launcher.app"

if not (do shell script "test -d " & quoted form of launcher & " && echo yes || echo no") is "yes" then
  display dialog "Could not find “7D2D Mac Launcher.app” next to this helper." & return & return & "Keep both apps in the same unzipped folder, then try again." buttons {"OK"} default button 1 with icon stop
  return
end if

try
  do shell script "xattr -cr " & quoted form of launcher
  try
    do shell script "codesign --force --deep --sign - " & quoted form of launcher
  end try
  do shell script "rm -rf " & quoted form of dest
  do shell script "ditto " & quoted form of launcher & " " & quoted form of dest
  do shell script "xattr -cr " & quoted form of dest
  try
    do shell script "codesign --force --deep --sign - " & quoted form of dest
  end try
on error errMsg
  display dialog "Couldn't prepare the app:" & return & return & errMsg buttons {"OK"} default button 1 with icon stop
  return
end try

try
  do shell script "open " & quoted form of dest
on error
  display dialog "Ready in Applications." & return & return & "Open Applications and double-click “7D2D Mac Launcher”." buttons {"OK"} default button 1
end try
APPLESCRIPT

osacompile -o "$PACK/Open Me First.app" "$TMP_AS"
rm -f "$TMP_AS"
codesign --force --deep --sign - "$PACK/Open Me First.app" 2>/dev/null || true
xattr -cr "$PACK/Open Me First.app" 2>/dev/null || true

cp "$ROOT/FRIEND-SETUP.md" "$PACK/HOW-TO-PLAY.md"

echo "==> Zipping…"
(
  cd "$OUT_DIR"
  ditto -c -k --keepParent "UndeadLegacy-Mac-Setup" "UndeadLegacy-Mac-Setup.zip"
)
xattr -cr "$ZIP" 2>/dev/null || true

echo
echo "Done:"
ls -lah "$ZIP" "$PACK"
echo
echo "Share: $ZIP"
