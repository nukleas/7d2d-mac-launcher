//! In-place Undead Legacy install for macOS (no full game clone).

use crate::paths::expand_user_path;
use crate::progress::{progress, progress_bytes};
use crate::steam::{detect_game, free_space_bytes, game_root_from_info};
use serde::Serialize;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;
use walkdir::WalkDir;
use zip::ZipArchive;

// setsid() for detaching the game process on macOS/Linux
#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Official Mod-Launcher zip endpoint for experimental (same catalog V5 uses).
const UL_EXP_URL: &str = "https://ul.subquake.com/dl/index.php?v=ml_exp";
/// Public experimental download.
const UL_EXP_URL_ALT: &str = "https://ul.subquake.com/dl?v=exp";

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

pub fn install_undead_legacy_experimental(
    app: &AppHandle,
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
            message: "We couldn’t find 7 Days to Die. Install it in Steam, then try again."
                .into(),
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

    if !info.looks_like_a20 && !force {
        progress(
            app,
            "error",
            "Wrong game version",
            "Switch Steam beta to alpha20.7",
            0,
        );
        return InstallResult {
            ok: false,
            message: "Undead Legacy needs Steam beta “alpha20.7”. In Steam: right-click 7 Days to Die → Properties → Betas → alpha20.7. Wait for the download, then try again."
                .into(),
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
        if free < 2_000_000_000 {
            progress(
                app,
                "error",
                "Not enough free space",
                "Need about 2 GB free",
                0,
            );
            return InstallResult {
                ok: false,
                message: "Your disk is almost full. Free at least 2 GB, then try again.".into(),
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

    let zip_path = cache.join("UndeadLegacy-Experimental.zip");
    progress(
        app,
        "download",
        "Downloading Undead Legacy…",
        "This can take a few minutes — please leave this window open",
        10,
    );
    steps.push("Downloading Undead Legacy Experimental…".into());

    let download_bytes = match download_first_ok(app, &[UL_EXP_URL, UL_EXP_URL_ALT], &zip_path) {
        Ok(n) => {
            steps.push(format!("Downloaded {}", friendly_bytes(n)));
            progress(
                app,
                "download",
                "Download complete",
                friendly_bytes(n),
                68,
            );
            Some(n)
        }
        Err(e) => {
            return fail(app, &game, steps, format!("Download failed: {e}"));
        }
    };

    let extract_dir = cache.join("ul-extract");
    let _ = fs::remove_dir_all(&extract_dir);
    if let Err(e) = fs::create_dir_all(&extract_dir) {
        return fail(
            app,
            &game,
            steps,
            format!("Could not create extract folder: {e}"),
        );
    }

    progress(
        app,
        "extract",
        "Unpacking files…",
        "Almost there",
        70,
    );
    steps.push("Unpacking…".into());
    if let Err(e) = extract_zip(app, &zip_path, &extract_dir) {
        return fail(app, &game, steps, format!("Unpack failed: {e}"));
    }

    let source_root = find_ul_root(&extract_dir).unwrap_or(extract_dir.clone());
    steps.push(format!("Package ready: {}", source_root.display()));

    // Note: Unity Doorstop uses "doorstop_*" (not "doorstep_*").
    // Missing doorstop_libs → dyld abort: libdoorstop_x64.dylib not found.
    // required=true → install fails if the package omits them (never silent skip).
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
                if let Err(e) = copy_item(&src, &game.join(name)) {
                    return fail(app, &game, steps, format!("Could not install {name}: {e}"));
                }
                steps.push(format!("Installed {name}"));
            }
            None if *required => {
                return fail(
                    app,
                    &game,
                    steps,
                    format!(
                        "The download is missing required file “{name}”. Try Install again, or download UL from ul.subquake.com."
                    ),
                );
            }
            None => steps.push(format!("Skipped optional: {name}")),
        }
    }

    let app_bundle = game.join("7DaysToDie.app");
    let mods_src = game.join("Mods");
    if mods_src.is_dir() && app_bundle.is_dir() {
        progress(
            app,
            "copy",
            "Finishing Mac setup…",
            "Copying mods into the game app",
            97,
        );
        let app_mods = app_bundle.join("Mods");
        if let Err(e) = copy_item(&mods_src, &app_mods) {
            steps.push(format!("Warning: could not copy Mods into app: {e}"));
        } else {
            steps.push("Copied Mods into 7DaysToDie.app".into());
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
    let doorstop_ok = game
        .join("doorstop_libs")
        .join("libdoorstop_x64.dylib")
        .is_file()
        || game
            .join("doorstop_libs")
            .join("libdoorstop_x86.dylib")
            .is_file();
    if !doorstop_ok {
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

    progress(
        app,
        "finish",
        "All set!",
        "You can press Play now",
        100,
    );

    InstallResult {
        ok: true,
        message: "Success! Press Play — the launcher starts the game for you (no Terminal or scripts needed)."
            .into(),
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
    let doorstop_x64 = game.join("doorstop_libs/libdoorstop_x64.dylib");
    let doorstop_x86 = game.join("doorstop_libs/libdoorstop_x86.dylib");
    let doorstop_lib = if doorstop_x64.is_file() {
        doorstop_x64
    } else if doorstop_x86.is_file() {
        doorstop_x86
    } else {
        return LaunchResult {
            ok: false,
            message: "Can't play yet — install is incomplete. Press Install again to repair, then Play."
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

    if !game.join("Mods/UndeadLegacy/UndeadLegacy.dll").is_file()
        && !game
            .join("7DaysToDie.app/Mods/UndeadLegacy/UndeadLegacy.dll")
            .is_file()
    {
        return LaunchResult {
            ok: false,
            message: "Can't play yet — Undead Legacy files are missing. Press Install again."
                .into(),
            command: String::new(),
        };
    }

    let app_bundle = game.join("7DaysToDie.app");
    let executable = resolve_mac_executable(&app_bundle).unwrap_or_else(|| {
        app_bundle.join("Contents/MacOS").join("7 Days To Die")
    });
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

/// Launch 7DTD with Doorstop injected and `-noeac` so UndeadLegacy.dll loads.
fn spawn_modded_game(
    game: &Path,
    executable: &Path,
    doorstop_lib: &Path,
    doorstop_libs: &Path,
    preloader: &Path,
    config: &Path,
) -> Result<String, String> {
    use std::process::Stdio;

    let log_path = game.join("ul_launcher_output.log");
    let log_file = File::create(&log_path).map_err(|e| format!("log file: {e}"))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("log clone: {e}"))?;

    let mut cmd = Command::new(executable);
    cmd.current_dir(game);
    cmd.env("DOORSTOP_ENABLE", "TRUE");
    cmd.env("DOORSTOP_INVOKE_DLL_PATH", preloader);
    cmd.env("DOORSTOP_CORLIB_OVERRIDE_PATH", "BepInEx/core");
    cmd.env_remove("DOORSTOP_DISABLE");
    // macOS injection (same as run_bepinex.sh, but we own -noeac).
    cmd.env("DYLD_LIBRARY_PATH", doorstop_libs);
    cmd.env("DYLD_INSERT_LIBRARIES", doorstop_lib);
    cmd.env("LD_LIBRARY_PATH", doorstop_libs);
    cmd.env(
        "LD_PRELOAD",
        doorstop_lib.file_name().unwrap_or_default(),
    );

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
        let snippet: String = tail.chars().rev().take(800).collect::<String>().chars().rev().collect();
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
) -> Result<String, String> {
    let game_s = shell_escape(&game.to_string_lossy());
    let exe_s = shell_escape(&executable.to_string_lossy());
    let lib_s = shell_escape(&doorstop_lib.to_string_lossy());
    let libs_s = shell_escape(&doorstop_libs.to_string_lossy());
    let pre_s = shell_escape(&preloader.to_string_lossy());
    let cfg_s = shell_escape(&config.to_string_lossy());

    let inner = format!(
        "cd {game_s} && \
export DOORSTOP_ENABLE=TRUE && \
export DOORSTOP_INVOKE_DLL_PATH={pre_s} && \
export DOORSTOP_CORLIB_OVERRIDE_PATH=BepInEx/core && \
unset DOORSTOP_DISABLE && \
export DYLD_LIBRARY_PATH={libs_s} && \
export DYLD_INSERT_LIBRARIES={lib_s} && \
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

fn fail(app: &AppHandle, game: &Path, steps: Vec<String>, message: String) -> InstallResult {
    progress(app, "error", "Something went wrong", &message, 0);
    InstallResult {
        ok: false,
        message,
        game_path: game.to_string_lossy().into_owned(),
        steps,
        download_bytes: None,
    }
}

fn download_first_ok(app: &AppHandle, urls: &[&str], dest: &Path) -> Result<u64, String> {
    let mut last_err = String::from("no urls");
    for (i, url) in urls.iter().enumerate() {
        progress(
            app,
            "download",
            "Downloading Undead Legacy…",
            format!("Trying download source {} of {}", i + 1, urls.len()),
            12 + (i as u8 * 2),
        );
        match download_file(app, url, dest) {
            Ok(n) if n > 10_000 => return Ok(n),
            Ok(n) => {
                last_err = format!("Source returned only {n} bytes (likely an error page)");
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn download_file(app: &AppHandle, url: &str, dest: &Path) -> Result<u64, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("7d2d-mac-launcher/0.1 (clean-room; macOS)")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total = resp.content_length();
    let mut file = File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = 0;
    let mut last_emit = 0u64;

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
            let pct = match total {
                Some(t) if t > 0 => 12 + ((done as f64 / t as f64) * 55.0) as u8,
                _ => 30,
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
                pct.min(67),
                done,
                total,
            );
        }
    }

    Ok(done)
}

fn extract_zip(app: &AppHandle, zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let len = archive.len().max(1);
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };

        if i % 25 == 0 || i + 1 == len {
            let pct = 70 + ((i as f64 / len as f64) * 14.0) as u8;
            let name = file.name().to_string();
            let short = name.rsplit('/').next().unwrap_or(&name);
            progress(
                app,
                "extract",
                "Unpacking files…",
                format!("{}/{} · {short}", i + 1, len),
                pct.min(84),
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

fn find_ul_root(extract_dir: &Path) -> Option<PathBuf> {
    if extract_dir.join("BepInEx").exists()
        || extract_dir.join("Mods").exists()
        || extract_dir.join("run_bepinex.sh").exists()
    {
        return Some(extract_dir.to_path_buf());
    }
    for entry in WalkDir::new(extract_dir).max_depth(3) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        let p = entry.path();
        if p.join("BepInEx").exists()
            || p.join("run_bepinex.sh").exists()
            || (p.join("Mods").exists() && p.join("doorstep_config.ini").exists())
        {
            return Some(p.to_path_buf());
        }
    }
    None
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
