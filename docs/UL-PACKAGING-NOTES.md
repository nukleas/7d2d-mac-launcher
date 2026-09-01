# Undead Legacy packaging — findings

Notes for anyone writing an Undead Legacy installer or updater. Everything here
was verified against the live packages between 2026-08-29 and 2026-08-31, mostly
by getting it wrong first. Shared in the hope it saves someone the same days.

This repo is MIT; take whatever is useful.

---

## Distribution

### What the catalog keys actually resolve to

| Key | Resolves to | Stable? |
|-----|-------------|---------|
| `?v=exp_part1` / `?v=exp_part2` | GitLab archive of `main` | **No** — follows the branch |
| `?v=stable` | GitLab archive of `main` (2.6 line) | No, but 2.6 hasn't moved since 2022 |
| `?v=mirror` | single ~7.4 GB zip | **No** — tracks latest |
| `?v=ml_exp` | a 978-byte `LICENSE.zip` | Dead. This is the old Mod-Launcher key |
| `?v=2.7.15` | a fixed ~7.4 GB zip | Yes, but see below |

**There is no general per-version URL.** `?v=2.7.15` exists; `?v=2.7.19` and
`?v=2.7.22` return HTML. Don't build on the pattern — it's incidental.

The only source with **stable contents** is the GitLab archive addressed by
commit SHA:

```
https://gitlab.com/subquakesgroup/UndeadLegacyExperimentalPart1/-/archive/<sha>/UndeadLegacyExperimentalPart1-<sha>.zip
```

Commit titles are the version (`Update v2.7.22`), so the commit list is a usable
release history:

```
https://gitlab.com/api/v4/projects/subquakesgroup%2FUndeadLegacyExperimentalPart1/repository/commits?ref_name=main
```

### Experimental is two archives, and they MERGE

Part 1 and Part 2 both populate `Mods/UndeadLegacy/`, including the same
`Resources/` directory. Part 1 carries 19 `.ulm` files, Part 2 carries 2
(`Subquake_Items.ulm`, `Subquake_Power.ulm`).

**Extract them into the same tree — do not replace the directory.** A copy that
wipes its destination first will silently delete half the mod, and the result
looks like a successful install.

Sizes: ~3.09 GiB + ~3.33 GiB ≈ 6.9 GB compressed. Contents are already-compressed
Unity bundles, so unpacked is about the same again. Budget ~14 GB of working
space if you keep the archives while unpacking.

### Two things that will bite a downloader

**GitLab ignores `Range`.** Verified on a pinned-SHA URL: requesting
`bytes=100-199` returned `200` and 102 MB. There is no resume — a dropped
transfer restarts from zero, on a 3 GB file. It also sends no `Content-Length`,
so progress bars have nothing to divide by.

**GitLab archives are not byte-reproducible.** Two downloads of the *same commit*
differed by 261 KB — zip metadata varies between generations, contents identical.

> If you plan to hash downloads, this is the thing to know. **You cannot pin a
> checksum to a GitLab archive.** Hash the extracted contents, or hash a copy you
> store yourself. We hash only our own mirror's stored files and trust GitLab via
> the commit SHA in the URL.

---

## Detecting what's installed

**`ModInfo.xml` lies.** It reads `<Version value="2.7.01"/>` in 2.7.19 *and*
2.7.22 — verified against both commits. It is not bumped per release.

But the game still logs the correct version:

```
INF [MODS]     Loaded Mod: UndeadLegacy (2.7.22)
```

because the real version lives in the **assembly metadata of
`Mods/UndeadLegacy/UndeadLegacy.dll`**, not in `ModInfo.xml`. On a 2.7.22
install:

```
$ strings -a Mods/UndeadLegacy/UndeadLegacy.dll | grep -oE '^2\.7\.[0-9]{2}$'
2.7.22
```

So version detection is straightforward, just not where you'd expect:

- **Read the version out of `UndeadLegacy.dll`** — one small file, no archive,
  no lookup table. Best option for "which version do you have".
- **Hash that dll** if you also want to detect a tampered or partial install.
- **Write your own marker** recording the commit SHA you installed, if you want
  something that survives whatever upstream does to its metadata next.

---

## Loader differences by version and platform

### UL 2.6 uses Doorstop 3, UL 2.7 uses Doorstop 4

They share **no** environment variable names:

| | Doorstop 3 (UL 2.6) | Doorstop 4 (UL 2.7) |
|---|---|---|
| enable | `DOORSTOP_ENABLE=TRUE` | `DOORSTOP_ENABLED=1` |
| assembly | `DOORSTOP_INVOKE_DLL_PATH` | `DOORSTOP_TARGET_ASSEMBLY` (absolute) |
| corlib | `DOORSTOP_CORLIB_OVERRIDE_PATH` | `DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE` |

**Getting this wrong fails silently.** The game starts, loads, and runs
completely unmodded — no crash, no error. Detect the generation from the loader
file present rather than assuming.

### Loader filenames changed

| | macOS | Linux | Windows |
|---|---|---|---|
| UL 2.6 | `libdoorstop_x64.dylib`, `libdoorstop_x86.dylib` | `libdoorstop_x64.so` | `winhttp.dll` |
| UL 2.7 | `libdoorstop.dylib` (universal x86_64+arm64) | `libdoorstop_x64.so` | `winhttp.dll` |

Note UL 2.7 also ships `libdoorstop_x84.so` — a typo, not a real architecture.

### UL 2.7's own `run_bepinex.sh` is broken on macOS

It computes `libdoorstop_${arch}.${ext}` → `libdoorstop_x64.dylib`, which **does
not exist in 2.7**. The script exits without launching. The Linux path is fine
(`libdoorstop_x64.so` is present).

### macOS: `Mods` must exist inside the .app bundle too

Two subsystems resolve `Mods` differently:

| Subsystem | Looks in |
|-----------|----------|
| mod loader | `<game>/Mods` |
| asset-bundle loader | `<game>/7DaysToDie.app/Mods` |

The bundle loader builds its path from the app bundle's `Data` directory
(`Data/Bundles/Standalone/../../../Mods`). On Windows and Linux the executable
sits at the game root so that arithmetic lands correctly; on macOS the executable
is in `Contents/MacOS` and `Data` is inside the bundle, so it lands *inside*
`7DaysToDie.app`.

Without it the mod loads fine and **every `.ulm` fails** with `Parent folder not
found` — 1095 in one session, mostly `Subquake_Sounds.ulm`, so the game runs with
no music. A **symlink** (`7DaysToDie.app/Mods -> ../Mods`) fixes it for free;
copying costs 3.4 GB and can drift.

### On macOS, how the game is started decides whether the mod loads

`7dLauncher.app`, Steam's Play button, and `run_bepinex.sh` all start the game
**without** injecting Doorstop. Symptom:

```
Could not load file or assembly 'BepInEx, Version=5.4.4.0'
Could not load file or assembly '0Harmony, Version=2.9.0.0'
```

plus `.ulm` "not found" errors — because the path redirect is itself a BepInEx
plugin. Users reasonably conclude a file is missing; it isn't.

On **Windows this doesn't apply**: `winhttp.dll` is loaded by the process
automatically, so any launcher works.

---

## Install mechanics

- **Delete the vanilla `Mods/0_TFP_Harmony`.** It breaks UL. Replace the whole
  `Mods` directory rather than merging into the game's own.
- **EasyAntiCheat must be off** (`-noeac`), client and server.
- **`unzip` refuses Part 2** with `invalid zip file with overlapped components
  (possible zip bomb)`. The archive is valid — confirmed with
  `UNZIP_DISABLE_ZIPBOMB_DETECTION=TRUE` and Python's `zipfile`. Either set that
  variable or use a different extractor.
- Parsing a zip's central directory proves it isn't truncated, but **not** that
  the middle is intact — the directory lives at the end of the file.

---

## Steam

- UL 2.7 needs game **`v2.6`**, a *named beta branch*. Steam's default is V3.2
  and no UL build supports it, so "no beta selected" is a failure state.
- UL 2.6 needs `alpha20.7`.
- `appmanifest_251570.acf` records the branch **twice**:

  ```
  UserConfig.BetaKey     = v2.6        <- what was requested
  MountedConfig.BetaKey  = alpha20.7   <- what is installed
  ```

  They differ while a branch switch is downloading. Reading the first as if it
  were the second means installing a mod for a game that isn't there yet. Naive
  key-matching returns whichever appears first in the file — the requested one.
- The Unity version in `7DaysToDie.app/Contents/Info.plist` distinguishes A20
  (2020.3.x) from everything newer, but **not** v2.6 from V3.2 — both are
  2022.3.x. `CFBundleShortVersionString` is `1.0` on every build.

---

## Multiplayer

7 Days to Die **refuses connections between a client and server on different mod
builds**. UL 2.7 shipped six builds in five days (2.7.14 → 2.7.19), and 2.7.22
the day after. "Install the latest" therefore means two friends who install a day
apart cannot play together.

Any updater aimed at a group needs a notion of *which build we've all agreed on*,
not just *what's newest*. We publish a small JSON of pinned commits that both the
launcher and the server read, so a bump is one edit and neither side can drift.

Note also that a UL update can invalidate saves — 2.7.22 requires a new game if
your save predates 2.7.15.
