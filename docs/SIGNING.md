# Mac signing & notarization (no menus every time)

You already have a **Developer ID Application** cert in the login keychain. The painful part is only **once**: store Apple notary credentials. After that, every release is one command.

## One-time setup (~5 minutes)

### 1. App-specific password (or API key)

**Option A — Apple ID + app password (simplest)**

1. https://appleid.apple.com → **Sign-In and Security** → **App-Specific Passwords**
2. Create one named e.g. `notarytool`
3. Copy the password (xxxx-xxxx-xxxx-xxxx)

**Option B — App Store Connect API key (better for CI)**

1. https://appstoreconnect.apple.com → **Users and Access** → **Integrations** → **Team Keys**
2. Generate a key with **Developer** access
3. Download `AuthKey_XXXXXXXXXX.p8` once (keep it private)
4. Note **Key ID** + **Issuer ID**

### 2. Store credentials in the keychain (never re-type)

```bash
# Option A
xcrun notarytool store-credentials "notary-profile" \
  --apple-id "YOUR_APPLE_ID@email.com" \
  --team-id "TEAMIDHERE" \
  --password "xxxx-xxxx-xxxx-xxxx"

# Option B (API key)
xcrun notarytool store-credentials "notary-profile" \
  --key "/path/to/AuthKey_XXXXXXXXXX.p8" \
  --key-id "XXXXXXXXXX" \
  --issuer "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

Confirm:

```bash
xcrun notarytool history --keychain-profile "notary-profile"
```

### 3. Local env file (gitignored)

```bash
cp .env.signing.example .env.signing
# edit .env.signing if you want overrides
```

`.env.signing` is **gitignored**. Never commit passwords or `.p8` keys.

---

## Every release (one command)

```bash
bun run release:signed
```

That will:

1. Build the release `.app` + `.dmg`
2. Sign with **Developer ID Application: Your Name Or Company (TEAMIDHERE)**
3. Notarize via `notarytool` using keychain profile `notary-profile`
4. Staple the ticket to the DMG/app
5. Build the friend zip (Open Me First + guide)

When notarization succeeds, friends generally **won’t** see “damaged” / Gatekeeper blocks.

---

## What Tauri needs (optional env)

| Variable | Purpose |
|----------|---------|
| `APPLE_SIGNING_IDENTITY` | Full cert name (script sets this) |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | Notary if not using keychain profile |
| `APPLE_API_KEY` / `APPLE_API_ISSUER` / `APPLE_API_KEY_PATH` | API-key notary for CI |

This project prefers **keychain profile** so you don’t put passwords in the shell.

---

## CI (GitHub Actions) later

Export the `.p8` + certs as GitHub secrets and use the same script. Local keychain profile does **not** exist on CI — use API key auth there.

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| `no identity found` | Open Keychain Access → ensure Developer ID cert is valid; run `security find-identity -v -p codesigning` |
| `notarytool` auth failed | Re-run `store-credentials`; app password, not your Apple ID login password |
| Notarization “Invalid” | Check email / `notarytool log <id> --keychain-profile notary-profile` |
| Still “damaged” after notarize | Staple (`stapler staple`) the **same** file you ship; don’t re-zip in a way that strips the ticket |

Team ID for this machine’s cert: **TEAMIDHERE**
