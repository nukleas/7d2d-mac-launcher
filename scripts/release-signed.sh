#!/usr/bin/env bash
# Sign + notarize (keychain profile) + staple + friend zip.
# Requires .env.signing (see .env.signing.example and docs/SIGNING.md).
# One-time: xcrun notarytool store-credentials "YOUR_PROFILE" ...
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ -f "$ROOT/.env.signing" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "$ROOT/.env.signing"
  set +a
fi

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "error: APPLE_SIGNING_IDENTITY is required." >&2
  echo "  cp .env.signing.example .env.signing  # then edit with your cert name" >&2
  echo "  See docs/SIGNING.md" >&2
  exit 1
fi
if [[ -z "${APPLE_TEAM_ID:-}" ]]; then
  echo "error: APPLE_TEAM_ID is required (10-char team id)." >&2
  exit 1
fi
if [[ -z "${NOTARY_PROFILE:-}" ]]; then
  echo "error: NOTARY_PROFILE is required (notarytool keychain profile name)." >&2
  exit 1
fi

IDENTITY="$APPLE_SIGNING_IDENTITY"
PROFILE="$NOTARY_PROFILE"
PRODUCT="7D2D Mac Launcher"
VERSION="$(node -p "require('./package.json').version")"

echo "==> Identity: $IDENTITY"
echo "==> Notary:   $PROFILE"
echo "==> Version:  $VERSION"

if ! security find-identity -v -p codesigning | grep -F "$IDENTITY" >/dev/null; then
  echo "error: signing identity not in keychain" >&2
  exit 1
fi
if ! xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1; then
  echo "error: notary profile missing. See docs/SIGNING.md" >&2
  exit 1
fi

export APPLE_SIGNING_IDENTITY="$IDENTITY"
export APPLE_TEAM_ID

echo "==> Build (codesign via Developer ID; notarize handled below)…"
# Avoid PIPESTATUS/zsh issues — capture exit explicitly
set +e
bun run package
PKG_EC=$?
set -e

APP="$ROOT/src-tauri/target/release/bundle/macos/${PRODUCT}.app"
if [[ ! -d "$APP" ]]; then
  echo "error: app not built (package exit $PKG_EC)" >&2
  exit 1
fi
echo "    package exit code: $PKG_EC (app present)"

echo "==> Deep re-sign…"
codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$APP"
codesign --verify --verbose=2 "$APP"

echo "==> Notarize .app…"
APP_ZIP="$ROOT/src-tauri/target/release/bundle/macos/${PRODUCT}-notarize.zip"
rm -f "$APP_ZIP"
ditto -c -k --keepParent "$APP" "$APP_ZIP"
xcrun notarytool submit "$APP_ZIP" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
rm -f "$APP_ZIP"

echo "==> Rebuild + notarize DMG from stapled app…"
mkdir -p "$ROOT/src-tauri/target/release/bundle/dmg"
DMG="$ROOT/src-tauri/target/release/bundle/dmg/${PRODUCT}_${VERSION}_aarch64.dmg"
STAGE=$(mktemp -d)
ditto "$APP" "$STAGE/${PRODUCT}.app"
rm -f "$DMG"
hdiutil create -volname "$PRODUCT" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"
codesign --force --timestamp --sign "$IDENTITY" "$DMG" || true
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

echo "==> Friend zip…"
OUT_DIR="$ROOT/dist-release"
PACK="$OUT_DIR/UndeadLegacy-Mac-Setup"
ZIP="$OUT_DIR/UndeadLegacy-Mac-Setup.zip"
rm -rf "$PACK" "$ZIP"
mkdir -p "$PACK"
ditto "$APP" "$PACK/7D2D Mac Launcher.app"
xattr -cr "$PACK/7D2D Mac Launcher.app" 2>/dev/null || true
cp "$ROOT/FRIEND-SETUP.md" "$PACK/HOW-TO-PLAY.md"

TMP_AS=$(mktemp /tmp/omf.XXXXXX.applescript)
cat > "$TMP_AS" <<'APPLESCRIPT'
set mePosix to POSIX path of (path to me as text)
set packFolder to do shell script "dirname " & quoted form of mePosix
set launcher to packFolder & "/7D2D Mac Launcher.app"
set dest to (POSIX path of (path to applications folder)) & "7D2D Mac Launcher.app"
if not (do shell script "test -d " & quoted form of launcher & " && echo yes || echo no") is "yes" then
  display dialog "Could not find 7D2D Mac Launcher.app next to this helper." buttons {"OK"} default button 1 with icon stop
  return
end if
try
  do shell script "xattr -cr " & quoted form of launcher
  do shell script "rm -rf " & quoted form of dest
  do shell script "ditto " & quoted form of launcher & " " & quoted form of dest
  do shell script "xattr -cr " & quoted form of dest
on error errMsg
  display dialog "Couldn't prepare: " & errMsg buttons {"OK"} default button 1 with icon stop
  return
end try
try
  do shell script "open " & quoted form of dest
on error
  display dialog "Installed to Applications." buttons {"OK"} default button 1
end try
APPLESCRIPT
osacompile -o "$PACK/Open Me First.app" "$TMP_AS"
rm -f "$TMP_AS"
codesign --force --deep --sign - "$PACK/Open Me First.app" 2>/dev/null || true
(
  cd "$OUT_DIR"
  ditto -c -k --keepParent "UndeadLegacy-Mac-Setup" "UndeadLegacy-Mac-Setup.zip"
)

echo
echo "======== DONE ========"
ls -lah "$APP" "$DMG" "$ZIP"
spctl -a -vv "$APP" 2>&1 || true
echo
echo "Publish:"
echo "  gh release upload v${VERSION} \"$ZIP\" \"$DMG\" --clobber"
