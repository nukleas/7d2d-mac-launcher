//! Locate the Steam install of 7 Days to Die on macOS.

use crate::paths::{default_steam_game_path, default_steam_manifest_path, expand_user_path};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub found: bool,
    pub game_path: String,
    pub app_path: String,
    pub manifest_path: String,
    pub beta_key: Option<String>,
    pub name: Option<String>,
    pub size_on_disk_bytes: Option<u64>,
    pub looks_like_a20: bool,
    pub has_bepinex: bool,
    pub has_run_bepinex: bool,
    /// Unity Doorstop dylibs required for Mac launch (`doorstop_libs/libdoorstop_*.dylib`).
    pub has_doorstop: bool,
    pub has_mods_folder: bool,
    /// True when BepInEx + doorstop + launch script + Mods are all present.
    pub mod_ready: bool,
    pub notes: Vec<String>,
}

pub fn detect_game(optional_path: Option<String>) -> GameInfo {
    let mut notes = Vec::new();

    let game_path = optional_path
        .filter(|s| !s.trim().is_empty())
        .map(|s| expand_user_path(&s))
        .unwrap_or_else(default_steam_game_path);

    let manifest_path = default_steam_manifest_path();
    let (beta_key, name, size_on_disk) = parse_manifest(&manifest_path);

    let app_path = game_path.join("7DaysToDie.app");
    let found = game_path.is_dir() && app_path.is_dir();

    if !found {
        notes.push(format!(
            "Game not found at {}. Install 7 Days to Die via Steam first.",
            game_path.display()
        ));
    }

    let looks_like_a20 = beta_key
        .as_deref()
        .map(|b| b.to_ascii_lowercase().contains("alpha20") || b.contains("20.7"))
        .unwrap_or(false)
        || unity_version_suggests_a20(&app_path);

    if found && !looks_like_a20 {
        notes.push(
            "Steam beta does not look like Alpha 20.7. Undead Legacy Experimental currently requires A20.7. \
             In Steam: right-click 7 Days to Die → Properties → Betas → alpha20.7."
                .into(),
        );
    }

    if found && looks_like_a20 {
        notes.push("Steam branch looks compatible with Undead Legacy (A20.7 era).".into());
    }

    let has_bepinex = game_path.join("BepInEx").is_dir();
    let has_run_bepinex = game_path.join("run_bepinex.sh").is_file();
    let has_doorstop = game_path
        .join("doorstop_libs")
        .join("libdoorstop_x64.dylib")
        .is_file()
        || game_path
            .join("doorstop_libs")
            .join("libdoorstop_x86.dylib")
            .is_file();
    let has_mods_folder = game_path.join("Mods").is_dir()
        || app_path.join("Mods").is_dir()
        || app_path.join("Contents").join("Mods").is_dir();

    // Fully playable: BepInEx + doorstop dylibs + Mods. Shell script optional (launcher injects itself).
    let mod_ready = has_bepinex && has_doorstop && has_mods_folder;

    if has_bepinex && !has_doorstop {
        notes.push(
            "Mod files are incomplete (missing doorstop). Click Install again to repair.".into(),
        );
    } else if mod_ready {
        notes.push("Undead Legacy looks fully installed and ready to play.".into());
    } else if has_bepinex {
        notes.push("BepInEx is present in the game folder.".into());
    }

    GameInfo {
        found,
        game_path: game_path.to_string_lossy().into_owned(),
        app_path: app_path.to_string_lossy().into_owned(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        beta_key,
        name,
        size_on_disk_bytes: size_on_disk,
        looks_like_a20,
        has_bepinex,
        has_run_bepinex,
        has_doorstop,
        has_mods_folder,
        mod_ready,
        notes,
    }
}

fn parse_manifest(path: &Path) -> (Option<String>, Option<String>, Option<u64>) {
    let Ok(text) = fs::read_to_string(path) else {
        return (None, None, None);
    };

    // VDF is simple key "value" pairs; good enough for BetaKey / name / SizeOnDisk.
    let beta = vdf_get(&text, "BetaKey");
    let name = vdf_get(&text, "name");
    let size = vdf_get(&text, "SizeOnDisk").and_then(|s| s.parse().ok());
    (beta, name, size)
}

fn vdf_get(text: &str, key: &str) -> Option<String> {
    // Matches: "BetaKey"  "alpha20.7"  (flexible whitespace)
    let needle = format!("\"{key}\"");
    for line in text.lines() {
        let t = line.trim();
        if !t.starts_with(&needle) {
            continue;
        }
        let rest = t[needle.len()..].trim();
        if let Some(inner) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
        // also: "key""value" with no space
        if let Some(stripped) = rest.strip_prefix('"') {
            if let Some(end) = stripped.find('"') {
                let v = &stripped[..end];
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn unity_version_suggests_a20(app_path: &Path) -> bool {
    // A20 Mac builds used Unity 2020.x; later branches moved on.
    let plist = app_path.join("Contents/Info.plist");
    let Ok(text) = fs::read_to_string(plist) else {
        return false;
    };
    text.contains("2020.3")
}

pub fn game_root_from_info(info: &GameInfo) -> PathBuf {
    PathBuf::from(&info.game_path)
}

/// Approximate free space on the volume containing `path`.
pub fn free_space_bytes(path: &Path) -> Option<u64> {
    use std::process::Command;
    let output = Command::new("df")
        .args(["-k", path.to_str()?])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Filesystem 1024-blocks Used Available Capacity ...
    let line = text.lines().nth(1)?;
    let avail_k: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_k * 1024)
}

