# 7D2D Mac Launcher

Clean-room, **macOS-first** installer for *7 Days to Die* overhauls — currently **Undead Legacy**, both release channels.

Built for people who just want to play (and for friends who should never have to open Terminal).

> **Not a fork of SphereII ModLauncher V5.** That Unity source is not public. This reimplements the Mac workflows we needed after hitting V5 path bugs, update loops, full-game clones, and broken launches on Sequoia.

## Why this exists

| Problem with V5 on Mac | What we do |
|------------------------|------------|
| Saves `~/…` literally → fake `~` folders | Always expand home paths to absolute paths |
| Self-update restart loop | No auto-updater |
| Clones a full second game (~14GB+) | **In-place** install into the Steam folder |
| Shell / BepInEx launch left to the user | **Play** injects Doorstop + **`-noeac`** for you |
| Sequoia “app is damaged” | Release zip includes **Open Me First** quarantine fix |
| Self-update restart loop | No self-updater; only the *build list* is fetched at run time |
| Leaves a ~30 GB cloned `Alpha20/` behind | Installs in place; nothing to clean up |
| Copies `Mods/` into the app bundle (3.4 GB) | Symlinked — the asset loader needs the path, not a second copy |

## Release channels

Subquake ships two Undead Legacy lines at once, against different base games.
Pick one in the app; it auto-selects the one matching your install.

| Channel | UL | Base game | Steam branch | Package |
|---------|----|-----------|--------------|---------|
| **Experimental** | 2.7.x | v2.6 | `v2.6` | two archives (~6.9 GB, needs ~14 GB free) |
| **Stable** | 2.6.x | Alpha 20.7 | `alpha20.7` | one archive (needs ~5 GB free) |

Both are Steam **beta** branches — Steam's default is V3.2, which no UL build
supports, so "no beta selected" is a failure state rather than a safe default.

Both are pinned to exact commits. 7 Days to Die refuses connections between a
client and server on different mod builds, so every player must install the same
one — see `server/README.md`.

The pins live in [`pinned-build.json`](./pinned-build.json), fetched at install
time. Bumping the mod is a one-line commit that every launcher picks up on its
next install — no reissued app, no reinstall. The server setup script reads the
same file, so the two cannot drift apart. If the file is unreachable or
malformed the launcher falls back to the pins compiled into the binary, so an
offline friend still gets a working install.

## Features (v0.3)

- Detect Steam install, and classify the game as A20 or v2.x from its Unity build  
- Per-channel version gate, free-space check and download set  
- Multi-part download → single staging tree → **rename** into the game folder  
- Retries when a large download drops, and reuses parts already fetched —
  GitLab ignores `Range`, so an interrupted transfer otherwise restarts from zero  
- Records the installed build, so pressing **Install** when you already have the
  pinned one does nothing instead of re-fetching 6.9 GB  
- **Play** launches with the right Doorstop generation + Easy Anti-Cheat off  
- Friend-friendly UI + `FRIEND-SETUP.md`  

## End-user (your friend)

1. Open **7D2D Mac Launcher** (use **Open Me First** if Mac says “damaged”)  
2. Pick which Undead Legacy you want — the app says which Steam branch it needs  
3. Steam → 7DTD → Betas → set that branch → wait  
4. **Install Undead Legacy** → wait  
5. **Play Undead Legacy** (every time — not Steam’s Play)  

See [FRIEND-SETUP.md](./FRIEND-SETUP.md).

## Develop

**Requirements:** macOS 11+ (Intel or Apple Silicon — the release is a universal binary), [Rust](https://rustup.rs/), [Bun](https://bun.sh/) (or npm), Xcode CLT, Steam + 7DTD.

```bash
git clone https://github.com/nukleas/7d2d-mac-launcher.git
cd 7d2d-mac-launcher
bun install
bun run tauri dev
```

```bash
# Rust unit tests
bun run test:rust

# Unsigned / ad-hoc release .app + .dmg
bun run package

# Friend zip (Open Me First + guide) — ad-hoc, may hit Sequoia Gatekeeper
bun run release-zip

# Developer ID sign + notarize + staple + friend zip (recommended)
# One-time: see docs/SIGNING.md  →  then:
bun run release:signed
```

Artifacts:

- App/DMG: `src-tauri/target/release/bundle/`  
- Zip: `dist-release/UndeadLegacy-Mac-Setup.zip`  

Signing details: [docs/SIGNING.md](./docs/SIGNING.md).
## Architecture (short)

| Layer | Role |
|-------|------|
| Tauri 2 + Rust | Steam detect, download, install, launch |
| Vite + TypeScript | Friendly UI, progress events |
| UL package | Official archives from Subquake, per channel |

Launch always sets:

- `DYLD_INSERT_LIBRARIES` → the Doorstop dylib in `doorstop_libs/`  
- the preloader path — but **the variable name depends on the payload**:
  UL 2.6 ships Doorstop 3 (`DOORSTOP_INVOKE_DLL_PATH`), UL 2.7 ships Doorstop 4
  (`DOORSTOP_TARGET_ASSEMBLY`). Using the wrong one is silent: the game starts
  unmodded rather than failing, so the generation is read off the installed
  dylib name.  
- **`-noeac -nogs`** so `UndeadLegacy.dll` loads (without this you get red XUi/texture spam)

## License & credits

- **MIT** — this project ([LICENSE](./LICENSE))  
- *7 Days to Die* © The Fun Pimps  
- *Undead Legacy* © Subquake — [ul.subquake.com](https://ul.subquake.com)  
- Not affiliated with SphereII / The7D2DModLauncher  

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Please keep the end-user path **no Terminal required**.
