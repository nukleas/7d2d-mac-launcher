//! In-place Undead Legacy install for macOS (no full game clone).

use crate::paths::expand_user_path;
use crate::steam::{detect_game, free_space_bytes, game_root_from_info};
use serde::Serialize;
use std::fs::{self, File};
use std::io::{self, copy};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use zip::ZipArchive;

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
    game_path_override: Option<String>,
    force: bool,
) -> InstallResult {
    let mut steps = Vec::new();
    let info = detect_game(game_path_override);
    let game = game_root_from_info(&info);

    if !info.found {
        return InstallResult {
            ok: false,
            message: "7 Days to Die not found. Install via Steam and set beta to alpha20.7."
                .into(),
            game_path: game.to_string_lossy().into_owned(),
            steps: info.notes,
            download_bytes: None,
        };
    }

    if !info.looks_like_a20 && !force {
        return InstallResult {
            ok: false,
            message: "Game does not look like Alpha 20.7. Switch Steam beta to alpha20.7, \
                      or pass force=true if you know what you're doing."
                .into(),
            game_path: game.to_string_lossy().into_owned(),
            steps: info.notes,
            download_bytes: None,
        };
    }

    // Space check: UL zip is small vs full clone; warn if < 2 GB free.
    if let Some(free) = free_space_bytes(&game) {
        steps.push(format!(
            "Free space on volume: {:.1} GB",
            free as f64 / 1_073_741_824.0
        ));
        if free < 2_000_000_000 {
            return InstallResult {
                ok: false,
                message: "Less than 2 GB free. Free some disk space before installing."
                    .into(),
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
        return fail(&game, steps, format!("Could not create cache dir: {e}"));
    }

    let zip_path = cache.join("UndeadLegacy-Experimental.zip");
    steps.push(format!("Downloading UL Experimental → {}", zip_path.display()));

    let download_bytes = match download_first_ok(&[UL_EXP_URL, UL_EXP_URL_ALT], &zip_path) {
        Ok(n) => {
            steps.push(format!("Downloaded {n} bytes"));
            Some(n)
        }
        Err(e) => {
            return fail(&game, steps, format!("Download failed: {e}"));
        }
    };

    let extract_dir = cache.join("ul-extract");
    let _ = fs::remove_dir_all(&extract_dir);
    if let Err(e) = fs::create_dir_all(&extract_dir) {
        return fail(&game, steps, format!("Could not create extract dir: {e}"));
    }

    steps.push("Extracting zip…".into());
    if let Err(e) = extract_zip(&zip_path, &extract_dir) {
        return fail(&game, steps, format!("Extract failed: {e}"));
    }

    // Zip may contain a single top-level folder (UndeadLegacy / similar).
    let source_root = find_ul_root(&extract_dir).unwrap_or(extract_dir.clone());
    steps.push(format!("UL package root: {}", source_root.display()));

    // In-place copy into Steam game folder — no full game clone.
    let copy_names = [
        "BepInEx",
        "doorstep_config.ini",
        "doorstep_libs",
        "Mods",
        "run_bepinex.sh",
        "winhttp.dll", // harmless if present; used on Windows
    ];

    for name in copy_names {
        let src = source_root.join(name);
        if !src.exists() {
            // also search one level deeper
            let mut found = None;
            if let Ok(rd) = fs::read_dir(&source_root) {
                for ent in rd.flatten() {
                    let p = ent.path().join(name);
                    if p.exists() {
                        found = Some(p);
                        break;
                    }
                }
            }
            if let Some(p) = found {
                if let Err(e) = copy_item(&p, &game.join(name)) {
                    return fail(&game, steps, format!("Copy {name} failed: {e}"));
                }
                steps.push(format!("Installed {name}"));
            } else {
                steps.push(format!("Skip missing package entry: {name}"));
            }
            continue;
        }
        if let Err(e) = copy_item(&src, &game.join(name)) {
            return fail(&game, steps, format!("Copy {name} failed: {e}"));
        }
        steps.push(format!("Installed {name}"));
    }

    // Mac guide: also place Mods next to / inside the .app package.
    let app = game.join("7DaysToDie.app");
    let mods_src = game.join("Mods");
    if mods_src.is_dir() && app.is_dir() {
        let app_mods = app.join("Mods");
        if let Err(e) = copy_item(&mods_src, &app_mods) {
            steps.push(format!("Warning: could not copy Mods into app bundle: {e}"));
        } else {
            steps.push("Copied Mods into 7DaysToDie.app/Mods".into());
        }
        // Some older guides used Contents/Resources; keep primary at app root Mods.
    }

    // chmod +x run_bepinex.sh
    let runner = game.join("run_bepinex.sh");
    if runner.is_file() {
        let _ = Command::new("chmod").args(["+x", runner.to_str().unwrap_or("")]).status();
        steps.push("chmod +x run_bepinex.sh".into());
    } else {
        steps.push(
            "run_bepinex.sh not found after install — launch may need a different entry point."
                .into(),
        );
    }

    // Clear quarantine on scripts/dylibs if present
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&game)
        .status();
    steps.push("Cleared quarantine xattrs on game folder (best effort)".into());

    InstallResult {
        ok: true,
        message: "Undead Legacy Experimental installed in-place (no full game clone). \
                  Launch with the Launch button (uses run_bepinex.sh, EAC off)."
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
            message: "Game not found.".into(),
            command: String::new(),
        };
    }

    let runner = game.join("run_bepinex.sh");
    if runner.is_file() {
        let cmd = format!("cd {} && ./run_bepinex.sh", shell_escape(&game.to_string_lossy()));
        // Open in Terminal so the user can see logs / allow Gatekeeper.
        let status = Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "tell application \"Terminal\" to do script \"cd {} && chmod +x ./run_bepinex.sh && ./run_bepinex.sh\"",
                shell_escape(&game.to_string_lossy())
            ))
            .status();

        return match status {
            Ok(s) if s.success() => LaunchResult {
                ok: true,
                message: "Started run_bepinex.sh in Terminal. Leave Terminal open while playing. \
                          If macOS blocks it: System Settings → Privacy & Security → Allow."
                    .into(),
                command: cmd,
            },
            Ok(s) => LaunchResult {
                ok: false,
                message: format!("Terminal launch failed with status {s}"),
                command: cmd,
            },
            Err(e) => LaunchResult {
                ok: false,
                message: format!("Could not open Terminal: {e}"),
                command: cmd,
            },
        };
    }

    // Fallback: open app without EAC launcher
    let app = game.join("7DaysToDie.app");
    let cmd = format!("open \"{}\"", app.display());
    match Command::new("open").arg(&app).status() {
        Ok(s) if s.success() => LaunchResult {
            ok: true,
            message: "Opened 7DaysToDie.app directly (no BepInEx script found). \
                      Mods may not load without BepInEx."
                .into(),
            command: cmd,
        },
        Ok(s) => LaunchResult {
            ok: false,
            message: format!("open failed: {s}"),
            command: cmd,
        },
        Err(e) => LaunchResult {
            ok: false,
            message: format!("open failed: {e}"),
            command: cmd,
        },
    }
}

fn fail(game: &Path, steps: Vec<String>, message: String) -> InstallResult {
    InstallResult {
        ok: false,
        message,
        game_path: game.to_string_lossy().into_owned(),
        steps,
        download_bytes: None,
    }
}

fn download_first_ok(urls: &[&str], dest: &Path) -> Result<u64, String> {
    let mut last_err = String::from("no urls");
    for url in urls {
        match download_file(url, dest) {
            Ok(n) if n > 10_000 => return Ok(n),
            Ok(n) => {
                last_err = format!("{url} returned only {n} bytes (likely an error page)");
            }
            Err(e) => last_err = format!("{url}: {e}"),
        }
    }
    Err(last_err)
}

fn download_file(url: &str, dest: &Path) -> Result<u64, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("7d2d-mac-launcher/0.1 (clean-room; macOS)")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let mut file = File::create(dest).map_err(|e| e.to_string())?;
    let n = copy(&mut resp, &mut file).map_err(|e| e.to_string())?;
    Ok(n)
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
            copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
        // best-effort unix mode
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

fn find_ul_root(extract_dir: &Path) -> Option<PathBuf> {
    // Prefer a directory that contains BepInEx or Mods or run_bepinex.sh
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
    // Minimal single-quote escape for AppleScript/shell.
    format!("'{}'", s.replace('\'', "'\\''"))
}
