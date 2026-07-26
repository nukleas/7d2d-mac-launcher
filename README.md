# 7D2D Mac Launcher

Clean-room, **macOS-first** installer/launcher for *7 Days to Die* overhauls.

This is **not** a fork of SphereII’s Unity ModLauncher V5 — that source is not public. It is a new open-source project that fixes the Mac problems we hit with V5:

| V5 pain on Mac | This project |
|----------------|--------------|
| Saves `~/…` literally → fake `~` folders | Always expands `~` / `$HOME` to absolute paths |
| Self-update loop (DMG tag ≠ bundle version) | No self-updater in v0.1 |
| Defaults to cloning a full game (~14GB+) | **In-place** install into Steam by default |
| Windows-oriented tooling (7-Zip portable, etc.) | Native `df`, `open`, Terminal + `run_bepinex.sh` |
| Hard to reason about partial installs | Explicit UL Experimental flow first |

## Status (v0.1)

- Detect Steam install + `appmanifest_251570.acf` beta branch  
- Disk free-space readout  
- **Undead Legacy Experimental** download + in-place install  
- Launch via `run_bepinex.sh` in Terminal (EAC off)  

Not yet: full overhaul catalog, modlets, multi-install profiles, Linux/Windows polish.

## Requirements

- macOS 11+  
- Steam + **7 Days to Die**  
- For Undead Legacy today: Steam beta **`alpha20.7`**  
- [Rust](https://rustup.rs/), [Bun](https://bun.sh/) (or npm), Xcode CLT  

## Develop

```bash
cd 7d2d-mac-launcher
bun install
bun run tauri dev
```

## Build .app / .dmg

```bash
bun run tauri build
```

Artifacts under `src-tauri/target/release/bundle/`.

## Undead Legacy notes

- Official downloads: [ul.subquake.com/download](https://ul.subquake.com/download)  
- This app uses the experimental package endpoints (`ml_exp` / `exp`)  
- Install is **in-place** under your Steam game folder (no second full copy)  
- First launch may need **Privacy & Security → Allow** for `run_bepinex.sh`  

## Why not a git fork?

Public GitHub for V5 is a **docs site** + release binaries. The Unity project path embedded in builds (`…/7D2DModLauncherV5/Assets/Scripts/…`) is not published. We reimplemented the Mac workflows we need.

## License

MIT (this project).  
7 Days to Die © The Fun Pimps. Undead Legacy © Subquake.  
Not affiliated with SphereII / The7D2DModLauncher.
