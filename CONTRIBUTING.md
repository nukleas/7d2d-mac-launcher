# Contributing

Thanks for helping. This project prioritizes **Mac friends who are not technical**.

## Ground rules

1. **No required Terminal steps** for normal install/play.  
2. Prefer **in-place** installs over full game clones.  
3. Paths must **expand `~` / `$HOME`** — never store bare tilde paths.  
4. Launch must pass **`-noeac`** (and Doorstop) so UL’s C# UI loads.  
5. Keep copy/UI language plain English.

## Dev setup

```bash
bun install
bun run tauri dev
bun run test:rust
```

## Pull requests

- Small, focused changes  
- Update `FRIEND-SETUP.md` / `README.md` if user-facing behavior changes  
- Bump version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` together when releasing  

## Release checklist

```bash
bun run test:rust
bun run package
bun run release-zip
```

Smoke-test on a clean Steam A20.7 install: Install → Play → confirm menu is UL (not red console spam) and log has `-noeac`.
