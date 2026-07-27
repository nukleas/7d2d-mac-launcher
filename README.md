# 7D2D Mac Launcher

Clean-room, **macOS-first** installer for *7 Days to Die* overhauls — starting with **Undead Legacy Experimental**.

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

## Features (v0.2)

- Detect Steam install + `appmanifest_251570.acf` beta branch  
- Free disk space readout  
- Progress bar install (download → unpack → copy) on a **background thread** (UI stays responsive)  
- **Undead Legacy Experimental** in-place install (`doorstop_*` files verified)  
- **Play** launches the game with Doorstop + Easy Anti-Cheat off (no shell scripts for end users)  
- Friend-friendly UI + `FRIEND-SETUP.md`  

## End-user (your friend)

1. Steam → 7DTD → Betas → **`alpha20.7`** → wait  
2. Open **7D2D Mac Launcher** (use **Open Me First** if Mac says “damaged”)  
3. **Install Undead Legacy** → wait  
4. **Play Undead Legacy** (every time — not Steam’s Play)  

See [FRIEND-SETUP.md](./FRIEND-SETUP.md).

## Develop

**Requirements:** macOS 11+, [Rust](https://rustup.rs/), [Bun](https://bun.sh/) (or npm), Xcode CLT, Steam + 7DTD.

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
| UL package | Official experimental zip from Subquake |

Launch always sets:

- `DYLD_INSERT_LIBRARIES` → `doorstop_libs/libdoorstop_*.dylib`  
- `DOORSTOP_INVOKE_DLL_PATH` → BepInEx preloader  
- **`-noeac -nogs`** so `UndeadLegacy.dll` loads (without this you get red XUi/texture spam)

## License & credits

- **MIT** — this project ([LICENSE](./LICENSE))  
- *7 Days to Die* © The Fun Pimps  
- *Undead Legacy* © Subquake — [ul.subquake.com](https://ul.subquake.com)  
- Not affiliated with SphereII / The7D2DModLauncher  

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Please keep the end-user path **no Terminal required**.
