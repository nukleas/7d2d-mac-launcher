#!/usr/bin/env bash
# Sign + notarize + staple + friend zip — no Keychain Access menus.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Optional local overrides
if [[ -f "$ROOT/.env.signing" ]]; then
  # shellcheck disable=SC1091
  set -a
  source "$ROOT/.env.signing"
  set +a
fi

IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: Your Name Or Company (TEAMIDHERE)}"
TEAM_ID="${APPLE_TEAM_ID:-TEAMIDHERE}"
PROFILE="${NOTARY_PROFILE:-notary-profile}"
PRODUCT="7D2D Mac Launcher"
VERSION="$(node -p "require('./package.json').version" 2>/dev/null || echo "0.0.0")"

echo "==> Signing identity: $IDENTITY"
echo "==> Notary profile:  $PROFILE"
echo "==> Version:         $VERSION"

# Sanity: identity present?
if ! security find-identity -v -p codesigning | grep -F "$IDENTITY" >/dev/null; then
  echo "error: signing identity not found in keychain:" >&2
  echo "  $IDENTITY" >&2
  echo "Run: security find-identity -v -p codesigning" >&2
  exit 1
fi

# Sanity: notary profile works?
if ! xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1; then
  echo "error: notarytool keychain profile \"$PROFILE\" missing or invalid." >&2
  echo "One-time setup (see docs/SIGNING.md):" >&2
  echo "  xcrun notarytool store-credentials \"$PROFILE\" --apple-id ... --team-id $TEAM_ID --password ..." >&2
  exit 1
fi

export APPLE_SIGNING_IDENTITY="$IDENTITY"
export APPLE_TEAM_ID="$TEAM_ID"

echo "==> Building (Tauri will codesign with Developer ID)…"
# Prefer keychain profile for notarization when Tauri supports env;
# we also notarize explicitly below for reliability.
bun run package

APP="$ROOT/src-tauri/target/release/bundle/macos/${PRODUCT}.app"
DMG="$ROOT/src-tauri/target/release/bundle/dmg/${PRODUCT}_${VERSION}_aarch64.dmg"

if [[ ! -d "$APP" ]]; then
  echo "error: missing app at $APP" >&2
  exit 1
fi

echo "==> Re-sign app deeply with Developer ID…"
codesign --force --deep --options runtime --timestamp \
  --sign "$IDENTITY" \
  "$APP"
codesign --verify --verbose=2 "$APP"

sign_and_notarize_dmg() {
  local dmg_path="$1"
  if [[ ! -f "$dmg_path" ]]; then
    # Try glob if version in filename differs
    dmg_path="$(ls -1 "$ROOT"/src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)"
  fi
  if [[ -z "${dmg_path:-}" || ! -f "$dmg_path" ]]; then
    echo "warn: no DMG found; notarizing .app zip instead"
    return 1
  fi

  echo "==> Notarizing DMG: $dmg_path"
  # Submit + wait
  xcrun notarytool submit "$dmg_path" \
    --keychain-profile "$PROFILE" \
    --wait

  echo "==> Stapling DMG…"
  xcrun stapler staple "$dmg_path"
  xcrun stapler validate "$dmg_path"
  echo "DMG ready: $dmg_path"
  echo "$dmg_path"
}

echo "==> Notarizing…"
# Notarize the .app via a zip (Apple accepts zip of app)
APP_ZIP="$ROOT/src-tauri/target/release/bundle/macos/${PRODUCT}-notarize.zip"
rm -f "$APP_ZIP"
ditto -c -k --keepParent "$APP" "$APP_ZIP"

xcrun notarytool submit "$APP_ZIP" \
  --keychain-profile "$PROFILE" \
  --wait

echo "==> Stapling .app…"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
rm -f "$APP_ZIP"

if DMG_PATH=$(sign_and_notarize_dmg "$DMG"); then
  :
else
  # Rebuild dmg from stapled app if needed
  echo "warn: DMG notarize skipped or failed path lookup"
fi

# Friend zip from already-signed app
echo "==> Building friend zip from stapled app…"
OUT_DIR="$ROOT/dist-release"
PACK="$OUT_DIR/UndeadLegacy-Mac-Setup"
ZIP="$OUT_DIR/UndeadLegacy-Mac-Setup.zip"
rm -rf "$PACK" "$ZIP"
mkdir -p "$PACK"
ditto "$APP" "$PACK/7D2D Mac Launcher.app"
# Keep stapled signature; do NOT ad-hoc re-sign
xattr -cr "$PACK/7D2D Mac Launcher.app" 2>/dev/null || true

# Open Me First still helps if someone re-quarantines; keep it
TMP_AS=$(mktemp /tmp/open_me_first.XXXXXX.applescript)
cat > "$TMP_AS" <<'APPLESCRIPT'
set mePosix to POSIX path of (path to me as text)
set packFolder to do shell script "dirname " & quoted form of mePosix
set launcher to packFolder & "/7D2D Mac Launcher.app"
set dest to (POSIX path of (path to applications folder)) & "7D2D Mac Launcher.app"
if not (do shell script "test -d " & quoted form of launcher & " && echo yes || echo no") is "yes" then
  display dialog "Could not find “7D2D Mac Launcher.app” next to this helper." buttons {"OK"} default button 1 with icon stop
  return
end if
try
  do shell script "xattr -cr " & quoted form of launcher
  do shell script "rm -rf " & quoted form of dest
  do shell script "ditto " & quoted form of launcher & " " & quoted form of dest
  do shell script "xattr -cr " & quoted form of dest
on error errMsg
  display dialog "Couldn't prepare the app:" & return & return & errMsg buttons {"OK"} default button 1 with icon stop
  return
end try
try
  do shell script "open " & quoted form of dest
on error
  display dialog "Installed to Applications. Open “7D2D Mac Launcher” from Applications." buttons {"OK"} default button 1
end try
APPLESCRIPT
osacompile -o "$PACK/Open Me First.app" "$TMP_AS"
rm -f "$TMP_AS"
# Helper can stay ad-hoc; it's tiny
codesign --force --deep --sign - "$PACK/Open Me First.app" 2>/dev/null || true

cp "$ROOT/FRIEND-SETUP.md" "$PACK/HOW-TO-PLAY.md"
(
  cd "$OUT_DIR"
  ditto -c -k --keepParent "UndeadLegacy-Mac-Setup" "UndeadLegacy-Mac-Setup.zip"
)

echo
echo "========================================"
echo " Signed + notarized release ready"
echo "========================================"
echo "App:  $APP"
ls -lah "$ROOT"/src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null || true
echo "Zip:  $ZIP"
echo
echo "Publish:"
echo "  gh release upload v${VERSION} \"$ZIP\" --clobber"
echo "  # or create a new release with the stapled dmg + zip"
