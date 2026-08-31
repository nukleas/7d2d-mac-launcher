//! In-place Undead Legacy install for macOS (no full game clone).

use crate::channel::{Channel, Doorstop};
use crate::paths::expand_user_path;
use crate::pin::{self, PinnedPart};
use crate::progress::{progress, progress_bytes, Progress};
use crate::steam::{detect_game, find_doorstop, free_space_bytes, game_root_from_info};
use serde::Serialize;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use zip::ZipArchive;

// setsid() for detaching the game process on macOS/Linux
#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Anything smaller than this is an error page or a stray LICENSE.zip, not a
/// mod package — the retired `?v=ml_exp` key serves exactly such a 978-byte file.
const MIN_PLAUSIBLE_ARCHIVE_BYTES: u64 = 1_000_000;

/// GitLab streams archives without a content-length and sometimes drops the
/// connection partway through a few-hundred-megabyte transfer.
const DOWNLOAD_ATTEMPTS: u32 = 4;

/// Position of the private mirror in `Channel::sources` — second, after
/// GitLab. Only a download from here has a digest we can check against.
const MIRROR_SOURCE_INDEX: usize = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub ok: bool,
    pub message: String,
    pub game_path: String,
    pub steps: Vec<String>,
    pub download_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub ok: bool,
    pub message: String,
    pub command: String,
}

pub fn install_undead_legacy(
    app: &Progress,
    channel: Channel,
    game_path_override: Option<String>,
    force: bool,
) -> InstallResult {
    let mut steps = Vec::new();
    progress(
        app,
        "check",
        "Checking your game…",
        "Looking for 7 Days to Die on this Mac",
        2,
    );

    let info = detect_game(game_path_override);
    let game = game_root_from_info(&info);

    if !info.found {
        progress(
            app,
            "error",
            "Game not found",
            "Install 7 Days to Die from Steam first",
            0,
        );
        return InstallResult {
            ok: false,
            message: "We couldn’t find 7 Days to Die. Install it in Steam, then try again.".into(),
            game_path: game.to_string_lossy().into_owned(),
            steps: info.notes,
            download_bytes: None,
        };
    }

    progress(
        app,
        "check",
        "Checking game version…",
        info.beta_key
            .clone()
            .unwrap_or_else(|| "default branch".into()),
        6,
    );

    if !channel.accepts(info.game_line) && !force {
        let updating = info.game_line == crate::channel::GameLine::Updating;
        progress(
            app,
            "error",
            if updating {
                "Steam is still updating"
            } else {
                "Wrong game version"
            },
            format!("{} needs game {}", channel.ul_label(), channel.game_label()),
            0,
        );
        return InstallResult {
            ok: false,
            message: if updating {
                // Installing now would drop a mod built for one game generation
                // on top of the other, and the result looks installed but is broken.
                format!(
                    "Steam hasn't finished downloading {}. The branch is selected, but the game files on disk are still the old version. Wait for Steam to finish, then press Install.",
                    channel.game_label()
                )
            } else {
                format!(
                    "{} needs 7 Days to Die {}. {}",
                    channel.ul_label(),
                    channel.game_label(),
                    channel.switch_hint()
                )
            },
            game_path: game.to_string_lossy().into_owned(),
            steps: info.notes,
            download_bytes: None,
        };
    }

    if let Some(free) = free_space_bytes(&game) {
        let free_gb = free as f64 / 1_073_741_824.0;
        steps.push(format!("Free space: {free_gb:.1} GB"));
        progress(
            app,
            "check",
            "Checking free space…",
            format!("{free_gb:.1} GB available"),
            8,
        );
        let needed = channel.required_free_bytes();
        if free < needed {
            let needed_gb = needed as f64 / 1_073_741_824.0;
            progress(
                app,
                "error",
                "Not enough free space",
                format!("Need about {needed_gb:.0} GB free"),
                0,
            );
            return InstallResult {
                ok: false,
                message: format!(
                    "Your disk is almost full. {} needs about {needed_gb:.0} GB free to download, unpack and install — you have {free_gb:.1} GB. Free some space, then try again.",
                    channel.ul_label()
                ),
                game_path: game.to_string_lossy().into_owned(),
                steps,
                download_bytes: None,
            };
        }
    }

    let cache = dirs::cache_dir()
        .unwrap_or_else(|| expand_user_path("~/Library/Caches"))
        .join("7d2d-mac-launcher");
    if let Err(e) = fs::create_dir_all(&cache) {
        return fail(
            app,
            &game,
            steps,
            format!("Could not create cache folder: {e}"),
        );
    }

    // Every part is unpacked into one staging tree, so the copy phase below
    // sees a single merged package no matter how many archives it arrived in.
    let staging = cache.join(format!("{}-staging", channel.cache_stem()));
    let _ = fs::remove_dir_all(&staging);
    if let Err(e) = fs::create_dir_all(&staging) {
        return fail(
            app,
            &game,
            steps,
            format!("Could not create staging folder: {e}"),
        );
    }

    // Which build to install is resolved at run time, so a mod bump reaches
    // everyone without reissuing the app. Falls back to the compiled-in pins.
    let (parts, pin_source) = pin::resolve(channel);
    steps.push(format!("Build list: {}", pin_source.describe()));
    progress(
        app,
        "check",
        "Checking which build to install…",
        pin_source.describe(),
        9,
    );
    let parts: &[PinnedPart] = &parts;
    let mut download_total: u64 = 0;

    for (index, part) in parts.iter().enumerate() {
        let span = PartSpan::new(index, parts.len());
        progress(
            app,
            "download",
            format!("Downloading {}…", channel.ul_label()),
            format!(
                "Getting {} — this can take a few minutes, please leave this window open",
                part.label
            ),
            span.download_start(),
        );
        // Record which build this is: the first question when someone can't
        // join a server is whether they're on the same one.
        steps.push(format!(
            "Installing {} — build {}",
            part.label,
            part.build_id()
        ));

        let zip_path = cache.join(format!("{}-{index}.zip", channel.cache_stem()));
        let mut from_mirror = false;

        // GitLab ignores Range headers, so a dropped transfer restarts from
        // zero — and part 1 is gigabytes. A complete archive from an earlier
        // attempt is therefore worth keeping and reusing.
        if let Some(cached) = complete_archive_size(&zip_path) {
            download_total += cached;
            steps.push(format!(
                "Reused {} already downloaded ({})",
                part.label,
                friendly_bytes(cached)
            ));
            progress(
                app,
                "download",
                "Already downloaded",
                format!("Reusing {}", part.label),
                span.extract_start(),
            );
        } else {
            let (bytes, source_used) =
                match download_pinned(app, &sources_for(channel, part, index), &zip_path, &span) {
                    Ok(pair) => pair,
                    Err(e) => {
                        return fail(
                            app,
                            &game,
                            steps,
                            format!("Download of {} failed: {e}", part.label),
                        );
                    }
                };
            download_total += bytes;
            steps.push(format!(
                "Downloaded {} ({}) from {}",
                part.label,
                friendly_bytes(bytes),
                if source_used == MIRROR_SOURCE_INDEX {
                    "the local mirror"
                } else {
                    "gitlab.com"
                }
            ));
            from_mirror = source_used == MIRROR_SOURCE_INDEX;
        }

        // Verify the mirror's copy against its recorded digest. This only
        // applies to a fresh mirror download: GitLab's archives are generated
        // per request and not byte-reproducible, so no fixed digest can
        // describe them, and a cached file's origin is no longer known. Catches
        // a mirror left holding a stale build after a version bump — which
        // would otherwise be an install that silently can't join the server.
        if let (true, Some(expected)) = (from_mirror, part.mirror_sha256.as_deref()) {
            progress(
                app,
                "extract",
                "Checking the download…",
                format!("Verifying {}", part.label),
                span.extract_start(),
            );
            match file_digest(&zip_path) {
                Ok(actual) if actual == expected => {
                    steps.push(format!("Verified {} ({})", part.label, &actual[..12]));
                }
                Ok(actual) => {
                    // Drop it so the next attempt re-fetches rather than
                    // reusing a file we've just proven wrong.
                    let _ = fs::remove_file(&zip_path);
                    return fail(
                        app,
                        &game,
                        steps,
                        format!(
                            "{} downloaded incorrectly — its checksum doesn't match the expected build \
                             (got {}…, expected {}…). It has been discarded; press Install again.",
                            part.label,
                            &actual[..12],
                            &expected[..12]
                        ),
                    );
                }
                Err(e) => {
                    return fail(
                        app,
                        &game,
                        steps,
                        format!("Could not verify {}: {e}", part.label),
                    );
                }
            }
        }

        progress(
            app,
            "extract",
            "Unpacking files…",
            format!("Unpacking {}", part.label),
            span.extract_start(),
        );

        // Straight into the shared tree: extract_zip strips each archive's
        // "<Repo>-main/" wrapper and merges, so parts that both carry Mods/
        // combine instead of overwriting one another.
        if let Err(e) = extract_zip(app, &zip_path, &staging, &span) {
            return fail(
                app,
                &game,
                steps,
                format!("Unpack of {} failed: {e}", part.label),
            );
        }
        steps.push(format!("Unpacked {}", part.label));
    }

    let download_bytes = Some(download_total);
    progress(
        app,
        "download",
        "Download complete",
        friendly_bytes(download_total),
        84,
    );

    // Each archive should have merged straight into the staging root. If one
    // kept its wrapper folder, the parts landed in separate subtrees and a
    // multi-part package would install only half of itself — fail rather than
    // quietly ship a broken mod.
    if !looks_like_ul_root(&staging) {
        return fail(
            app,
            &game,
            steps,
            "The download did not unpack into the expected layout. Try Install again, or download \
             Undead Legacy manually from ul.subquake.com."
                .into(),
        );
    }
    let source_root = staging.clone();
    steps.push(format!("Package ready: {}", source_root.display()));

    // Note: Unity Doorstop uses "doorstop_*" (not "doorstep_*").
    // Missing doorstop_libs → dyld abort: libdoorstop_x64.dylib not found.
    // required=true → install fails if the package omits them (never silent skip).
    // Each entry REPLACES its destination — install_item deletes first. That is
    // required, not incidental: the vanilla game ships Mods/0_TFP_Harmony, which
    // breaks Undead Legacy if it survives alongside it. Merging Mods/ instead of
    // replacing it would leave the game unplayable, so do not "fix" this.
    let copy_items: &[(&str, bool)] = &[
        ("BepInEx", true),
        ("doorstop_config.ini", true),
        ("doorstop_libs", true),
        ("Mods", true),
        ("run_bepinex.sh", false), // optional; we launch without requiring it
        ("winhttp.dll", false),    // Windows-only
    ];
    let total_copy = copy_items.len() as f32;

    for (i, (name, required)) in copy_items.iter().enumerate() {
        let pct = 85 + ((i as f32 / total_copy) * 12.0) as u8;
        progress(
            app,
            "copy",
            "Installing into your game…",
            format!("Copying {name}"),
            pct.min(97),
        );

        let resolved = resolve_package_entry(&source_root, name);
        match resolved {
            Some(src) => {
                if let Err(e) = install_item(&src, &game.join(name)) {
                    return fail(app, &game, steps, format!("Could not install {name}: {e}"));
                }
                steps.push(format!("Installed {name}"));
            }
            None if *required => {
                // Name the folder we actually looked in. Reported as "a file
                // was missing but it was totally there", because the reader
                // checks their *game* folder while this is about the freshly
                // downloaded package — two different directories.
                return fail(
                    app,
                    &game,
                    steps,
                    format!(
                        "The downloaded Undead Legacy package is incomplete — “{name}” isn't in it. \
                         (This is about the download, not your game folder. Looked in {}.) \
                         The download was probably cut short: press Install again to re-fetch it.",
                        source_root.display()
                    ),
                );
            }
            None => steps.push(format!("Skipped optional: {name}")),
        }
    }

    // Mods live at the game root only. We launch with the game root as the
    // working directory and doorstop_config.ini resolves `baseDir = Mods/`
    // from there, so a second copy inside 7DaysToDie.app is never read — it
    // just doubled a multi-gigabyte folder. Clear out any left by older
    // installs.
    let stale_app_mods = game.join("7DaysToDie.app/Mods");
    if stale_app_mods.is_dir() {
        progress(
            app,
            "copy",
            "Tidying up…",
            "Removing a duplicate mods folder from earlier installs",
            97,
        );
        match fs::remove_dir_all(&stale_app_mods) {
            Ok(()) => steps.push("Removed duplicate Mods copy inside 7DaysToDie.app".into()),
            Err(e) => steps.push(format!(
                "Note: could not remove the duplicate Mods copy: {e}"
            )),
        }
    }

    let runner = game.join("run_bepinex.sh");
    if runner.is_file() {
        let _ = Command::new("chmod")
            .args(["+x", runner.to_str().unwrap_or("")])
            .status();
        steps.push("Made launch script runnable".into());
    } else {
        return fail(
            app,
            &game,
            steps,
            "Install finished but the Play script (run_bepinex.sh) is missing. Try Install again."
                .into(),
        );
    }

    // Hard requirement for Mac: doorstop injects BepInEx. Without this, dyld aborts on launch.
    if find_doorstop(&game).is_none() {
        return fail(
            app,
            &game,
            steps,
            "Install is incomplete: doorstop_libs is missing. Click Install again (repair). \
             If it keeps failing, free some disk space and retry."
                .into(),
        );
    }
    steps.push("Verified doorstop launch libraries".into());

    if !game.join("BepInEx").is_dir() {
        return fail(
            app,
            &game,
            steps,
            "Install is incomplete: BepInEx folder is missing. Click Install again.".into(),
        );
    }

    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&game)
        .status();
    steps.push("Cleared macOS quarantine flags".into());

    // Only now that the install has landed: drop the staged tree and the
    // downloaded archives. Keeping them until this point means a failure
    // part-way through doesn't cost another multi-gigabyte download.
    let _ = fs::remove_dir_all(&staging);
    for index in 0..parts.len() {
        let _ = fs::remove_file(cache.join(format!("{}-{index}.zip", channel.cache_stem())));
    }

    progress(app, "finish", "All set!", "You can press Play now", 100);

    let build_ids = parts
        .iter()
        .map(|p| p.build_id())
        .collect::<Vec<_>>()
        .join(" + ");

    InstallResult {
        ok: true,
        message: format!(
            "Success! Press Play — the launcher starts the game for you (no Terminal or scripts needed).\n\nInstalled build: {build_ids}. Everyone on your server must be on this same build."
        ),
        game_path: game.to_string_lossy().into_owned(),
        steps,
        download_bytes,
    }
}

pub fn launch_undead_legacy(game_path_override: Option<String>) -> LaunchResult {
    let info = detect_game(game_path_override);
    let game = game_root_from_info(&info);
    if !info.found {
        return LaunchResult {
            ok: false,
            message: "Game not found. Install 7 Days to Die from Steam first.".into(),
            command: String::new(),
        };
    }

    // Pre-flight — friend never fixes this by hand; Install repairs it.
    let Some((doorstop_generation, doorstop_lib)) = find_doorstop(&game) else {
        return LaunchResult {
            ok: false,
            message:
                "Can't play yet — install is incomplete. Press Install again to repair, then Play."
                    .into(),
            command: String::new(),
        };
    };

    if !game.join("BepInEx/core/BepInEx.Preloader.dll").is_file() {
        return LaunchResult {
            ok: false,
            message: "Can't play yet — BepInEx is missing. Press Install again to repair.".into(),
            command: String::new(),
        };
    }

    if !game.join("Mods/UndeadLegacy/UndeadLegacy.dll").is_file() {
        return LaunchResult {
            ok: false,
            message: "Can't play yet — Undead Legacy files are missing. Press Install again."
                .into(),
            command: String::new(),
        };
    }

    let app_bundle = game.join("7DaysToDie.app");
    let executable = resolve_mac_executable(&app_bundle)
        .unwrap_or_else(|| app_bundle.join("Contents/MacOS").join("7 Days To Die"));
    if !executable.is_file() {
        return LaunchResult {
            ok: false,
            message: "Could not find the game executable inside 7DaysToDie.app.".into(),
            command: String::new(),
        };
    }

    let config = game.join("serverconfig.xml");
    let doorstop_libs = game.join("doorstop_libs");
    let preloader = game.join("BepInEx/core/BepInEx.Preloader.dll");

    // Clear quarantine so macOS doesn’t block dylib injection for friends.
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&doorstop_libs)
        .status();
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&executable)
        .status();

    // Primary: launch binary ourselves with Doorstop + -noeac (no shell scripts for the user).
    match spawn_modded_game(
        &game,
        &executable,
        &doorstop_lib,
        &doorstop_libs,
        &preloader,
        &config,
        doorstop_generation,
    ) {
        Ok(cmd_desc) => LaunchResult {
            ok: true,
            message: "Game is starting with Undead Legacy (Easy Anti-Cheat off). Wait for the menu — first load can take a minute."
                .into(),
            command: cmd_desc,
        },
        Err(e) => {
            // Fallback still one-click; Terminal only if direct spawn fails.
            match spawn_via_terminal_fallback(
                &game,
                &executable,
                &doorstop_lib,
                &doorstop_libs,
                &preloader,
                &config,
                doorstop_generation,
            ) {
                Ok(cmd_desc) => LaunchResult {
                    ok: true,
                    message: "Game is starting. If a Terminal window appears, leave it open while you play."
                        .into(),
                    command: format!("direct failed ({e}); fallback: {cmd_desc}"),
                },
                Err(e2) => LaunchResult {
                    ok: false,
                    message: format!(
                        "Could not start the game.\n\nDirect launch: {e}\nBackup launch: {e2}\n\nPress Install again, then try Play."
                    ),
                    command: String::new(),
                },
            }
        }
    }
}

/// Environment that makes Doorstop inject BepInEx.
///
/// Doorstop 3 and 4 share almost no variable names — UL 2.6 reads
/// `DOORSTOP_INVOKE_DLL_PATH`, UL 2.7 reads `DOORSTOP_TARGET_ASSEMBLY`. Getting
/// this wrong fails *silently*: the game starts clean instead of refusing to
/// start, so the mod simply never appears. The generation is therefore taken
/// from the dylib actually on disk rather than assumed.
fn doorstop_env(
    generation: Doorstop,
    preloader: &Path,
    doorstop_libs: &Path,
    doorstop_lib: &Path,
) -> Vec<(String, String)> {
    let preloader = preloader.display().to_string();
    let mut env = match generation {
        Doorstop::V3 => vec![
            ("DOORSTOP_ENABLE".to_string(), "TRUE".to_string()),
            ("DOORSTOP_INVOKE_DLL_PATH".to_string(), preloader),
            (
                "DOORSTOP_CORLIB_OVERRIDE_PATH".to_string(),
                "BepInEx/core".to_string(),
            ),
        ],
        Doorstop::V4 => vec![
            ("DOORSTOP_ENABLED".to_string(), "1".to_string()),
            // Doorstop 4 does not resolve this relative to the game folder.
            ("DOORSTOP_TARGET_ASSEMBLY".to_string(), preloader),
            ("DOORSTOP_IGNORE_DISABLED_ENV".to_string(), "0".to_string()),
            (
                "DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE".to_string(),
                String::new(),
            ),
            ("DOORSTOP_MONO_DEBUG_ENABLED".to_string(), "0".to_string()),
        ],
    };

    // Injection itself is identical across generations. Absolute paths here,
    // where run_bepinex.sh leans on DYLD_LIBRARY_PATH to resolve a bare name.
    let libs = doorstop_libs.display().to_string();
    env.push(("DYLD_LIBRARY_PATH".to_string(), libs.clone()));
    env.push((
        "DYLD_INSERT_LIBRARIES".to_string(),
        doorstop_lib.display().to_string(),
    ));
    env.push(("LD_LIBRARY_PATH".to_string(), libs));
    env.push((
        "LD_PRELOAD".to_string(),
        doorstop_lib
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    ));
    env
}

/// Launch 7DTD with Doorstop injected and `-noeac` so UndeadLegacy.dll loads.
fn spawn_modded_game(
    game: &Path,
    executable: &Path,
    doorstop_lib: &Path,
    doorstop_libs: &Path,
    preloader: &Path,
    config: &Path,
    generation: Doorstop,
) -> Result<String, String> {
    use std::process::Stdio;

    let log_path = game.join("ul_launcher_output.log");
    let log_file = File::create(&log_path).map_err(|e| format!("log file: {e}"))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("log clone: {e}"))?;

    let mut cmd = Command::new(executable);
    cmd.current_dir(game);
    for (key, value) in doorstop_env(generation, preloader, doorstop_libs, doorstop_lib) {
        cmd.env(key, value);
    }
    cmd.env_remove("DOORSTOP_DISABLE");

    // Critical: without -noeac the game refuses UndeadLegacy.dll → red XUi/texture spam.
    cmd.arg("-noeac");
    cmd.arg("-nogs");
    if config.is_file() {
        cmd.arg(format!("-configfile={}", config.display()));
    }

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(log_file));
    cmd.stderr(Stdio::from(log_err));

    // Detach so the launcher stays responsive and the game keeps running.
    #[cfg(unix)]
    {
        // SAFETY: setsid detaches from our process group; fine for a game client.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
    let pid = child.id();

    // Brief settle: if the process dies instantly, surface failure instead of false success.
    std::thread::sleep(std::time::Duration::from_millis(800));
    let still_alive = is_pid_alive(pid);
    if !still_alive {
        let tail = fs::read_to_string(&log_path).unwrap_or_default();
        let snippet: String = tail
            .chars()
            .rev()
            .take(800)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return Err(format!(
            "game exited immediately after launch. Last log lines:\n{snippet}"
        ));
    }

    // Record how we launched (helps support screenshots / friend debugging).
    let _ = fs::write(
        game.join("ul_launcher_last_run.txt"),
        format!(
            "pid={pid}\nexe={}\nargs=-noeac -nogs\ndoorstop={}\nlog={}\n",
            executable.display(),
            doorstop_lib.display(),
            log_path.display()
        ),
    );

    Ok(format!(
        "{} -noeac -nogs (pid {}, log {})",
        executable.display(),
        pid,
        log_path.display()
    ))
}

fn is_pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// One-click Terminal fallback: bakes env + -noeac so the user never types anything.
fn spawn_via_terminal_fallback(
    game: &Path,
    executable: &Path,
    doorstop_lib: &Path,
    doorstop_libs: &Path,
    preloader: &Path,
    config: &Path,
    generation: Doorstop,
) -> Result<String, String> {
    let game_s = shell_escape(&game.to_string_lossy());
    let exe_s = shell_escape(&executable.to_string_lossy());
    let cfg_s = shell_escape(&config.to_string_lossy());

    // Built from the same source as the direct spawn so the two cannot drift.
    let exports: String = doorstop_env(generation, preloader, doorstop_libs, doorstop_lib)
        .into_iter()
        .map(|(key, value)| format!("export {key}={} && ", shell_escape(&value)))
        .collect();

    let inner = format!(
        "cd {game_s} && \
{exports}\
unset DOORSTOP_DISABLE && \
exec {exe_s} -noeac -nogs -configfile={cfg_s}"
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            r#"tell application "Terminal"
  activate
  do script "{}"
end tell"#,
            inner.replace('\\', "\\\\").replace('"', "\\\"")
        ))
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(inner)
    } else {
        Err(format!("Terminal status {status}"))
    }
}

fn resolve_mac_executable(app_bundle: &Path) -> Option<PathBuf> {
    let plist = app_bundle.join("Contents/Info.plist");
    let output = Command::new("defaults")
        .args(["read", &plist.to_string_lossy(), "CFBundleExecutable"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(app_bundle.join("Contents/MacOS").join(name))
}

fn fail(app: &Progress, game: &Path, steps: Vec<String>, message: String) -> InstallResult {
    progress(app, "error", "Something went wrong", &message, 0);
    InstallResult {
        ok: false,
        message,
        game_path: game.to_string_lossy().into_owned(),
        steps,
        download_bytes: None,
    }
}

/// Fetch one pinned archive, trying each source in turn.
///
/// Multiple sources are only safe because the result is checksum-verified by
/// the caller: whichever host answers, the bytes are proven to be the pinned
/// build before anything is installed.
fn download_pinned(
    app: &Progress,
    sources: &[String],
    dest: &Path,
    span: &PartSpan,
) -> Result<(u64, usize), String> {
    let mut last_err = String::from("download did not run");
    for (i, url) in sources.iter().enumerate() {
        if i > 0 {
            progress(
                app,
                "download",
                "Trying another source…",
                format!("Source {} of {}", i + 1, sources.len()),
                span.download_start(),
            );
            // A partial from a source that turned out to be unreachable is not
            // resumable against a different host.
            let _ = fs::remove_file(dest);
        }
        for attempt in 1..=DOWNLOAD_ATTEMPTS {
            if attempt > 1 {
                progress(
                    app,
                    "download",
                    "Retrying download…",
                    format!("The connection dropped — attempt {attempt} of {DOWNLOAD_ATTEMPTS}"),
                    span.download_start(),
                );
            }
            match download_file(app, url, dest, span) {
                Ok(n) if n >= MIN_PLAUSIBLE_ARCHIVE_BYTES => return Ok((n, i)),
                Ok(n) => {
                    last_err =
                        format!("source returned only {n} bytes (an error page, not the mod)");
                    break;
                }
                Err(e) => last_err = e,
            }
        }
    }
    Err(last_err)
}

/// Where to fetch one part from, best first.
///
/// GitLab leads on speed; the private mirror follows as the resumable retry,
/// and is only offered when the manifest published a digest for it — an
/// unverifiable mirror is worse than no mirror.
fn sources_for(channel: Channel, part: &PinnedPart, index: usize) -> Vec<String> {
    let mut sources = vec![part.url()];
    if part.mirror_sha256.is_some() {
        sources.push(channel.mirror_url(index));
    }
    sources
}

/// SHA-256 of a file, streamed so a multi-gigabyte archive never lands in RAM.
fn file_digest(path: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn download_file(app: &Progress, url: &str, dest: &Path, span: &PartSpan) -> Result<u64, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("7d2d-mac-launcher/0.1 (clean-room; macOS)")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Resume where a previous attempt stopped, if the server allows it. The
    // private mirror answers 206; GitLab ignores Range and answers 200, in
    // which case we start over — so this is an optimisation, never a
    // correctness assumption.
    let already = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    let mut request = client.get(url);
    if already > 0 {
        request = request.header("Range", format!("bytes={already}-"));
    }

    let mut resp = request.send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        return Err(format!("HTTP {}", resp.status()));
    }

    let resuming = resp.status().as_u16() == 206;
    if resuming {
        progress(
            app,
            "download",
            "Resuming download…",
            format!("Continuing from {}", friendly_bytes(already)),
            span.download_start(),
        );
    }

    // 206 means the body continues from `already`; anything else is the whole
    // file again and the partial has to go.
    let total = resp
        .content_length()
        .map(|n| n + if resuming { already } else { 0 });
    let mut file = if resuming {
        fs::OpenOptions::new()
            .append(true)
            .open(dest)
            .map_err(|e| e.to_string())?
    } else {
        File::create(dest).map_err(|e| e.to_string())?
    };
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = if resuming { already } else { 0 };
    let mut last_emit = done;

    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;

        // Throttle UI updates (~every 256 KB)
        if done - last_emit >= 256 * 1024 || total.is_some_and(|t| done >= t) {
            last_emit = done;
            // GitLab streams archives with no content-length, so an unknown
            // total is normal here, not a failure.
            let pct = match total {
                Some(t) if t > 0 => span.download_pct(done as f32 / t as f32),
                _ => span.download_start(),
            };
            let detail = match total {
                Some(t) => format!("{} / {}", friendly_bytes(done), friendly_bytes(t)),
                None => format!("{} downloaded…", friendly_bytes(done)),
            };
            progress_bytes(
                app,
                "download",
                "Downloading Undead Legacy…",
                detail,
                pct,
                done,
                total,
            );
        }
    }

    Ok(done)
}

/// Size of `path` if it is a complete, readable zip; `None` otherwise.
///
/// A truncated download still opens as a file but fails to parse: the zip
/// central directory lives at the end, so this rejects partial archives.
fn complete_archive_size(path: &Path) -> Option<u64> {
    let size = fs::metadata(path).ok()?.len();
    if size < MIN_PLAUSIBLE_ARCHIVE_BYTES {
        return None;
    }
    let file = File::open(path).ok()?;
    ZipArchive::new(file).ok().map(|_| size)
}

/// The single top-level directory an archive wraps everything in, if any.
///
/// GitLab serves `UndeadLegacyExperimentalPart1-main/…`; stripping that lets
/// several archives extract straight into one shared tree.
fn archive_root_prefix(archive: &mut ZipArchive<File>) -> Option<PathBuf> {
    let mut root: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let file = archive.by_index(i).ok()?;
        let path = file.enclosed_name()?;
        let first = path.components().next()?.as_os_str();
        // A file sitting at the top level means there is no wrapper to strip.
        if path.components().count() == 1 && !file.name().ends_with('/') {
            return None;
        }
        match &root {
            Some(seen) if seen.as_os_str() != first => return None,
            Some(_) => {}
            None => root = Some(PathBuf::from(first)),
        }
    }
    root
}

/// Unpack `zip_path` into `dest`, merging with whatever is already there.
fn extract_zip(
    app: &Progress,
    zip_path: &Path,
    dest: &Path,
    span: &PartSpan,
) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let root_prefix = archive_root_prefix(&mut archive);
    let len = archive.len().max(1);
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = match file.enclosed_name() {
            Some(p) => p,
            None => continue,
        };
        let relative = match &root_prefix {
            Some(prefix) => match name.strip_prefix(prefix) {
                Ok(stripped) if stripped.as_os_str().is_empty() => continue,
                Ok(stripped) => stripped.to_path_buf(),
                Err(_) => name,
            },
            None => name,
        };
        let outpath = dest.join(relative);

        if i % 25 == 0 || i + 1 == len {
            let pct = span.extract_pct(i as f32 / len as f32);
            let name = file.name().to_string();
            let short = name.rsplit('/').next().unwrap_or(&name);
            progress(
                app,
                "extract",
                "Unpacking files…",
                format!("{}/{} · {short}", i + 1, len),
                pct,
            );
        }

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
            io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                let _ = fs::set_permissions(&outpath, fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

fn resolve_package_entry(source_root: &Path, name: &str) -> Option<PathBuf> {
    let direct = source_root.join(name);
    if direct.exists() {
        return Some(direct);
    }
    if let Ok(rd) = fs::read_dir(source_root) {
        for ent in rd.flatten() {
            let p = ent.path().join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// Progress percentages carved up between a channel's parts, so a two-archive
/// install advances smoothly instead of restarting the bar for each one.
pub struct PartSpan {
    index: usize,
    count: usize,
}

impl PartSpan {
    pub fn new(index: usize, count: usize) -> Self {
        Self {
            index,
            count: count.max(1),
        }
    }

    /// Downloads occupy 10..70, extraction 70..84.
    fn slice(&self, from: f32, to: f32) -> (f32, f32) {
        let width = (to - from) / self.count as f32;
        let start = from + width * self.index as f32;
        (start, start + width)
    }

    fn download_start(&self) -> u8 {
        self.slice(10.0, 70.0).0 as u8
    }

    fn extract_start(&self) -> u8 {
        self.slice(70.0, 84.0).0 as u8
    }

    /// Map this part's extraction progress (0.0..1.0) onto the overall bar.
    fn extract_pct(&self, fraction: f32) -> u8 {
        let (start, end) = self.slice(70.0, 84.0);
        (start + (end - start) * fraction.clamp(0.0, 1.0)) as u8
    }

    /// Map this part's download completion (0.0..1.0) onto the overall bar.
    fn download_pct(&self, fraction: f32) -> u8 {
        let (start, end) = self.slice(10.0, 70.0);
        (start + (end - start) * fraction.clamp(0.0, 1.0)) as u8
    }
}

/// Does this directory look like the root of a UL package?
///
/// Part 2 of the experimental package carries nothing but `Mods/`, so a
/// bare `Mods` directory has to count on its own.
fn looks_like_ul_root(p: &Path) -> bool {
    p.join("BepInEx").exists() || p.join("run_bepinex.sh").exists() || p.join("Mods").exists()
}

/// Install one staged entry into the game folder.
///
/// The staging tree is disposable, so a rename is preferred: it is instant and
/// needs no extra space, which matters when the package is several gigabytes.
/// Falls back to copying when the cache and the game sit on different volumes.
fn install_item(src: &Path, dest: &Path) -> io::Result<()> {
    if dest.exists() {
        if dest.is_dir() {
            fs::remove_dir_all(dest)?;
        } else {
            fs::remove_file(dest)?;
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => copy_item(src, dest),
    }
}

fn copy_item(src: &Path, dest: &Path) -> io::Result<()> {
    if src.is_dir() {
        if dest.exists() {
            fs::remove_dir_all(dest)?;
        }
        fs::create_dir_all(dest)?;
        for entry in WalkDir::new(src) {
            let entry = entry?;
            let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
            let target = dest.join(rel);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
        Ok(())
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn friendly_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let n = n as f64;
    if n >= GB {
        format!("{:.2} GB", n / GB)
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.0} KB", n / KB)
    } else {
        format!("{n:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(generation: Doorstop) -> Vec<(String, String)> {
        doorstop_env(
            generation,
            Path::new("/game/BepInEx/core/BepInEx.Preloader.dll"),
            Path::new("/game/doorstop_libs"),
            Path::new("/game/doorstop_libs/libdoorstop.dylib"),
        )
    }

    fn get(env: &[(String, String)], key: &str) -> Option<String> {
        env.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.to_string())
    }

    #[test]
    fn v4_uses_doorstop4_variable_names() {
        let env = env_of(Doorstop::V4);
        assert_eq!(get(&env, "DOORSTOP_ENABLED").as_deref(), Some("1"));
        assert_eq!(
            get(&env, "DOORSTOP_TARGET_ASSEMBLY").as_deref(),
            Some("/game/BepInEx/core/BepInEx.Preloader.dll")
        );
        // The Doorstop 3 spelling must not leak through, or 2.7 launches vanilla.
        assert!(get(&env, "DOORSTOP_INVOKE_DLL_PATH").is_none());
        assert!(get(&env, "DOORSTOP_ENABLE").is_none());
    }

    #[test]
    fn v3_uses_doorstop3_variable_names() {
        let env = env_of(Doorstop::V3);
        assert_eq!(get(&env, "DOORSTOP_ENABLE").as_deref(), Some("TRUE"));
        assert_eq!(
            get(&env, "DOORSTOP_INVOKE_DLL_PATH").as_deref(),
            Some("/game/BepInEx/core/BepInEx.Preloader.dll")
        );
        assert!(get(&env, "DOORSTOP_TARGET_ASSEMBLY").is_none());
        assert!(get(&env, "DOORSTOP_ENABLED").is_none());
    }

    /// The generations must not accidentally converge on a shared spelling —
    /// that is precisely the bug that makes the mod silently not load.
    #[test]
    fn generations_share_no_doorstop_variable() {
        let v3: Vec<String> = env_of(Doorstop::V3)
            .into_iter()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with("DOORSTOP_"))
            .collect();
        let v4: Vec<String> = env_of(Doorstop::V4)
            .into_iter()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with("DOORSTOP_"))
            .collect();
        for key in &v3 {
            assert!(!v4.contains(key), "{key} set by both generations");
        }
    }

    #[test]
    fn both_generations_inject_the_dylib_by_absolute_path() {
        for generation in [Doorstop::V3, Doorstop::V4] {
            let env = env_of(generation);
            assert_eq!(
                get(&env, "DYLD_INSERT_LIBRARIES").as_deref(),
                Some("/game/doorstop_libs/libdoorstop.dylib")
            );
            assert_eq!(
                get(&env, "DYLD_LIBRARY_PATH").as_deref(),
                Some("/game/doorstop_libs")
            );
        }
    }

    #[test]
    fn single_part_spans_the_whole_bar() {
        let span = PartSpan::new(0, 1);
        assert_eq!(span.download_pct(0.0), 10);
        assert_eq!(span.download_pct(1.0), 70);
    }

    #[test]
    fn two_parts_split_the_bar_without_overlapping() {
        let first = PartSpan::new(0, 2);
        let second = PartSpan::new(1, 2);
        assert_eq!(first.download_pct(0.0), 10);
        assert_eq!(first.download_pct(1.0), 40);
        assert_eq!(second.download_pct(0.0), 40);
        assert_eq!(second.download_pct(1.0), 70);
        assert!(second.extract_start() >= first.extract_start());
    }

    #[test]
    fn progress_never_runs_backwards_or_past_its_stage() {
        for index in 0..2 {
            let span = PartSpan::new(index, 2);
            assert!(span.download_pct(0.0) >= 10);
            assert!(span.download_pct(1.0) <= 70);
            assert!(span.extract_pct(1.0) <= 84);
            // Out-of-range fractions are clamped, not wrapped.
            assert_eq!(span.download_pct(-1.0), span.download_pct(0.0));
            assert_eq!(span.download_pct(9.0), span.download_pct(1.0));
        }
    }

    #[test]
    fn a_bare_mods_folder_counts_as_a_package_root() {
        // Part 2 of the experimental package contains nothing else.
        let dir = std::env::temp_dir().join("ul-test-part2-root");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Mods")).unwrap();
        assert!(looks_like_ul_root(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Full install against the live Subquake/GitLab archives.
    ///
    /// Downloads a few hundred megabytes, so it is ignored by default:
    ///   cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture
    ///
    /// This is the only check that exercises the real URLs, the archive
    /// root-strip, the two-part merge and the rename-install together.
    #[test]
    #[ignore = "downloads the real Undead Legacy package"]
    fn installs_experimental_from_the_real_archives() {
        let game = std::env::temp_dir().join("ul-e2e-game");
        let _ = fs::remove_dir_all(&game);
        // detect_game only requires a 7DaysToDie.app directory to consider the
        // game present; `force` skips the version gate for this fake root.
        fs::create_dir_all(game.join("7DaysToDie.app/Contents/MacOS")).unwrap();

        let result = install_undead_legacy(
            &Progress::silent(),
            Channel::Experimental,
            Some(game.to_string_lossy().into_owned()),
            true,
        );
        assert!(
            result.ok,
            "install failed: {}\nsteps: {:#?}",
            result.message, result.steps
        );

        // Part 1 payload.
        assert!(game.join("BepInEx/core/BepInEx.Preloader.dll").is_file());
        assert!(game.join("doorstop_libs/libdoorstop.dylib").is_file());
        assert!(game.join("Mods/UndeadLegacy/UndeadLegacy.dll").is_file());
        assert!(game.join("run_bepinex.sh").is_file());

        // The merge proof: these three .ulm files live in *different* archives,
        // and all must survive. A destructive per-directory copy would drop one
        // side, because both parts populate Mods/UndeadLegacy/Resources.
        let resources = game.join("Mods/UndeadLegacy/Resources");
        assert!(
            resources.join("Subquake_Core.ulm").is_file(),
            "part 1 resource missing — part 2 overwrote part 1"
        );
        for from_part_two in ["Subquake_Items.ulm", "Subquake_Power.ulm"] {
            assert!(
                resources.join(from_part_two).is_file(),
                "part 2 resource {from_part_two} missing — parts did not merge"
            );
        }

        // The installed package must be launchable as Doorstop 4.
        let (generation, _) = find_doorstop(&game).expect("doorstop not detected");
        assert_eq!(generation, Doorstop::V4);

        // Staging must not be left behind.
        let staging = dirs::cache_dir()
            .unwrap()
            .join("7d2d-mac-launcher")
            .join(format!("{}-staging", Channel::Experimental.cache_stem()));
        assert!(!staging.exists(), "staging tree left in the cache");

        let _ = fs::remove_dir_all(&game);
    }
}
