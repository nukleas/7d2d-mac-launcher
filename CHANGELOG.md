# Changelog

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
