# Mac signing & notarization (no menus every time)

If you have a **Developer ID Application** certificate in your login keychain, you only need to store Apple notary credentials **once**. After that, every release is one command.

## One-time setup (~5 minutes)

### 1. Find your signing identity

```bash
security find-identity -v -p codesigning
```

Copy the line that looks like:

```text
Developer ID Application: Your Name or Company (TEAMID)
```

Your **Team ID** is the 10-character code in parentheses.

### 2. App-specific password (or API key)

**Option A — Apple ID + app password (simplest)**

1. https://appleid.apple.com → **Sign-In and Security** → **App-Specific Passwords**
2. Create one named e.g. `notarytool`
3. Copy the password (format `xxxx-xxxx-xxxx-xxxx`)

**Option B — App Store Connect API key (better for CI)**

1. https://appstoreconnect.apple.com → **Users and Access** → **Integrations** → **Team Keys**
2. Generate a key with **Developer** access
3. Download `AuthKey_XXXXXXXXXX.p8` once (keep it private — never commit)
4. Note **Key ID** + **Issuer ID**

### 3. Store credentials in the keychain

```bash
# Option A
xcrun notarytool store-credentials "notary-profile" \
  --apple-id "YOUR_APPLE_ID@email.com" \
  --team-id "YOUR_TEAM_ID" \
  --password "xxxx-xxxx-xxxx-xxxx"

# Option B (API key)
xcrun notarytool store-credentials "notary-profile" \
  --key "/path/to/AuthKey_XXXXXXXXXX.p8" \
  --key-id "YOUR_KEY_ID" \
  --issuer "YOUR_ISSUER_UUID"
```

Confirm:

```bash
xcrun notarytool history --keychain-profile "notary-profile"
```

### 4. Local env file (gitignored)

```bash
cp .env.signing.example .env.signing
# fill in YOUR identity, team id, and profile name
```

**Never commit** `.env.signing`, `.p8` keys, or app-specific passwords.

---

## Every release (one command)

```bash
bun run release:signed
```

Requires `.env.signing` (or the same variables exported in your shell). The script will:

1. Build the release `.app` + `.dmg`
2. Sign with your Developer ID identity
3. Notarize via `notarytool` + keychain profile
4. Staple the ticket
5. Build the friend zip (Open Me First + guide)

When notarization succeeds, Gatekeeper usually accepts the app without “damaged” warnings.

---

## Environment variables

| Variable | Required | Purpose |
|----------|----------|---------|
| `APPLE_SIGNING_IDENTITY` | yes | Exact string from `security find-identity` |
| `APPLE_TEAM_ID` | yes | 10-char team id |
| `NOTARY_PROFILE` | yes | Name passed to `notarytool store-credentials` |
| `APPLE_ID` / `APPLE_PASSWORD` | no | Only if not using a keychain profile |
| `APPLE_API_KEY` / `APPLE_API_ISSUER` / `APPLE_API_KEY_PATH` | no | API-key notary (CI) |

This project prefers a **keychain profile** so passwords never live in shell history or the repo.

---

## CI (GitHub Actions)

Local keychain profiles do not exist on CI. Use an App Store Connect API key + exported Developer ID cert as **GitHub Actions secrets**. Never put those values in the public tree.

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| `APPLE_SIGNING_IDENTITY is required` | Create `.env.signing` from the example |
| `no identity found` | Cert missing/expired; re-check Keychain + `security find-identity` |
| `notarytool` auth failed | Re-run `store-credentials`; use an **app-specific** password |
| Notarization “Invalid” | `notarytool log <id> --keychain-profile YOUR_PROFILE` |
| Still “damaged” after notarize | Staple the **same** file you ship; don’t re-package without re-stapling |
| 403 agreement missing | Accept pending agreements in [App Store Connect](https://appstoreconnect.apple.com) / [developer.apple.com](https://developer.apple.com/account) |
