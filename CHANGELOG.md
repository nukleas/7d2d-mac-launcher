# Changelog

## 0.4.1

### Fixed
- **A version bump could silently install the previous build.** Cached archives
  were named `<channel>-<index>.zip` with no build in the name, and reuse only
  checked "is this a valid zip of a plausible size". After a pin bump, the
  perfectly valid archive from the *old* build matched every check and was
  installed while the log reported the new one — a successful install that still
  cannot join the server, with nothing explaining why. Cache files are now keyed
  by build id.

### Changed
- The current build's archives are **kept** after a successful install rather
  than deleted, and older pins pruned. Undead Legacy usually only moves part 1,
  so the next update downloads ~3.3 GB instead of the whole ~6.9 GB package. The
  cost is ~6.9 GB resident in `~/Library/Caches/7d2d-mac-launcher`, which is safe
  to delete at any time.

## 0.4.0

### Fixed
- **No sound, and a thousand asset errors.** 0.3.0 deleted `Mods` from inside
  `7DaysToDie.app`, on the evidence that the game writes `ModSettings.ini` to the
  copy at the game root. That was true but incomplete: the *mod loader* reads the
  game root, while the *asset-bundle loader* builds its path from the app
  bundle's `Data` directory and lands inside `7DaysToDie.app`. Removing that
  folder left every `.ulm` failing with "Parent folder not found" — 1095 of them
  in one session, mostly `Subquake_Sounds.ulm`, so no music.

  It is now a **symlink** rather than the 3.4 GB duplicate 0.2.0 copied in: same
  path resolution, no extra disk, and it cannot drift from the real folder.

  This is macOS-specific. On Windows and Linux the executable sits at the game
  root, so the same path arithmetic already lands in the right place.

### Added
- **Universal binary** — Intel Macs are supported again. The release script
  refuses to publish a build that isn't both `x86_64` and `arm64`.
- The game is launched with `-logfile`, so its structured log is captured
  alongside the launcher's stdout. Without it the only usable diagnostics came
  from BepInEx's own log, which made us harder to support than the game's own
  launcher.

## Unreleased

### Features
- **Runtime build list** (`pinned-build.json`): which Undead Legacy build to
  install is fetched at install time instead of compiled in, so a mod bump
  reaches every player without reissuing the app. Falls back to the built-in
  pins when unreachable or malformed. `server/setup-ul-server.sh` reads the same
  file, so server and clients cannot drift apart.
- **Private mirror fallback**: archives can also be served from a tailnet host.
  GitLab stays the primary because it is faster off that LAN; the mirror is the
  retry, because it honours `Range` and GitLab does not — an interrupted
  multi-gigabyte download resumes instead of restarting.
- Downloads resume where a previous attempt stopped, when the source allows it.
- Mirror downloads are checksum-verified before install.

### Fixes
- `unzip` refuses UL part 2 as a suspected zip bomb ("overlapped components")
  despite it being a valid archive; the server script now disables that check.
- Clearer message when the *downloaded package* is incomplete — it named a
  missing file without saying it meant the download, not the game folder.

### Notes
- GitLab archives are not byte-reproducible: two downloads of the same commit
  differed by 261 KB of zip metadata with identical contents. Checksums can
  therefore only describe a stored copy, not "the archive for commit X".

## 0.3.0

Undead Legacy 2.7 shipped for game v2.6 on 2026-08-17, ending the four-year
A20.7 freeze. Every part of the old install path had broken against it.

### Features
- **Release channels**: stable (UL 2.6.x / game A20.7) and experimental
  (UL 2.7.x / game v2.6), picked in the UI and auto-selected from the game found
- Multi-part packages: experimental ships as two archives that merge into one tree
- Downloads retry and fall back to a second mirror per part
- Install renames the staged tree into place instead of copying it

### Fixes
- **Dead download URL**: `?v=ml_exp` now serves a 978-byte `LICENSE.zip`;
  replaced with the current per-channel catalog keys and GitLab fallbacks
- **Inverted version gate**: the launcher hard-blocked anything that wasn't
  A20.7 and told users to downgrade — exactly wrong for UL 2.7
- **Silent launch failure**: UL 2.7 ships Doorstop 4, which renamed every
  environment variable. The old names left the game starting *unmodded* with no
  error. The generation is now read off the installed dylib
- **Renamed dylib**: UL 2.7 ships one universal `libdoorstop.dylib` (x86_64 +
  arm64) where 2.6 shipped `libdoorstop_x64/x86.dylib`; both are recognised
- Game version detected from the bundle's Unity build, since
  `CFBundleShortVersionString` reads `1.0` on both A20.7 and v2.x
- Stopped duplicating `Mods/` into `7DaysToDie.app` — several GB that the game
  never reads; existing copies are removed on install
- Free-space check is per channel instead of a flat 2 GB
- Staging tree is deleted after a successful install

## 0.2.0

### Features
- Background-thread install (UI no longer freezes on Install)
- Live progress events (download bytes, unpack, copy stages)
- Direct Play launch: Doorstop injection + **`-noeac`** (no shell scripts for users)
- Terminal fallback only if direct spawn fails
- Install verification for `doorstop_libs` / BepInEx / UL DLL
- Friend UI: step rail, checklist, sequoia-oriented guide

### Fixes
- Correct package names: `doorstop_*` (was wrongly `doorstep_*`)
- Absolute Steam paths (no literal `~` folders)
- Critical package entries cannot be silently skipped

### Distribution
- `scripts/make-release-zip.sh` builds zip with **Open Me First** Gatekeeper helper
- `FRIEND-SETUP.md` end-user guide

## 0.1.0 – 0.1.1

- Initial Tauri scaffold, UL experimental in-place install, basic detection
