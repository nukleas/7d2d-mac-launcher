//! Locate the Steam install of 7 Days to Die on macOS.

use crate::channel::{
    channel_for, line_from_beta_key, unity_says_alpha20, Channel, Doorstop, GameLine,
};
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
    /// Which base-game generation is installed — decides the usable UL channel.
    pub game_line: GameLine,
    /// The UL channel that matches `game_line`, used to preselect the UI.
    pub suggested_channel: Option<Channel>,
    pub has_bepinex: bool,
    pub has_run_bepinex: bool,
    /// Unity Doorstop dylib for Mac launch. UL 2.6 ships `libdoorstop_x64.dylib`,
    /// UL 2.7 a single universal `libdoorstop.dylib`; either counts.
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
    let manifest = parse_manifest(&manifest_path);
    // Surface the branch that is actually playable, not the one being fetched.
    let beta_key = manifest
        .installed_branch
        .clone()
        .or_else(|| manifest.requested_branch.clone());

    let app_path = game_path.join("7DaysToDie.app");
    let found = game_path.is_dir() && app_path.is_dir();

    if !found {
        notes.push(format!(
            "Game not found at {}. Install 7 Days to Die via Steam first.",
            game_path.display()
        ));
    }

    let game_line = if found {
        detect_game_line(&app_path, &manifest)
    } else {
        GameLine::Unknown
    };
    let suggested_channel = channel_for(game_line);

    match suggested_channel {
        Some(channel) => notes.push(format!(
            "Game looks like {} — install {}.",
            channel.game_label(),
            channel.ul_label()
        )),
        None if game_line == GameLine::Updating => notes.push(
            "Steam has switched branch but hasn't finished downloading it — the game files on \
             disk are still the old version. Let Steam finish, then check again."
                .into(),
        ),
        None if found => notes.push(
            "Could not tell which version of the game this is. Undead Legacy needs either \
             Alpha 20.7 (for UL 2.6) or v2.6 (for UL 2.7)."
                .into(),
        ),
        None => {}
    }

    let has_bepinex = game_path.join("BepInEx").is_dir();
    let has_run_bepinex = game_path.join("run_bepinex.sh").is_file();
    let has_doorstop = find_doorstop(&game_path).is_some();
    // Mods belong at the game root; copies inside the .app bundle are never read.
    let has_mods_folder = game_path.join("Mods").is_dir();

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
        name: manifest.name.clone(),
        size_on_disk_bytes: manifest.size_on_disk,
        game_line,
        suggested_channel,
        has_bepinex,
        has_run_bepinex,
        has_doorstop,
        has_mods_folder,
        mod_ready,
        notes,
    }
}

/// What the manifest says about the app's branch.
pub struct Manifest {
    /// `MountedConfig.BetaKey` — the branch whose files are on disk *now*.
    pub installed_branch: Option<String>,
    /// `UserConfig.BetaKey` — the branch Steam has been asked to install.
    /// Differs from `installed_branch` while a switch is still downloading.
    pub requested_branch: Option<String>,
    pub name: Option<String>,
    pub size_on_disk: Option<u64>,
}

fn parse_manifest(path: &Path) -> Manifest {
    let Ok(text) = fs::read_to_string(path) else {
        return Manifest {
            installed_branch: None,
            requested_branch: None,
            name: None,
            size_on_disk: None,
        };
    };

    // BetaKey appears twice — once under UserConfig (requested) and once under
    // MountedConfig (installed). Reading whichever comes first conflates
    // "what Steam is fetching" with "what you can actually play".
    Manifest {
        installed_branch: vdf_get_in(&text, "MountedConfig", "BetaKey"),
        requested_branch: vdf_get_in(&text, "UserConfig", "BetaKey"),
        name: vdf_get(&text, "name"),
        size_on_disk: vdf_get(&text, "SizeOnDisk").and_then(|s| s.parse().ok()),
    }
}

/// Read `key` from inside the `section { ... }` block only.
fn vdf_get_in(text: &str, section: &str, key: &str) -> Option<String> {
    let needle = format!("\"{section}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let open = rest.find('{')? + 1;
    let body = &rest[open..];
    let end = body.find('}')?;
    vdf_get(&body[..end], key)
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

/// Work out which base-game build is actually playable right now.
///
/// Steam records the branch twice: `UserConfig` is what you picked, and
/// `MountedConfig` is what is installed. Picking a branch flips the first
/// immediately while the second only changes after a ~15 GB download, so a
/// mismatch means "still updating" — and installing a mod for the requested
/// branch onto the mounted one produces a broken game that looks fine.
///
/// The Unity runtime is a fallback for when the manifest can't be read. It can
/// only separate A20 from everything newer, since v2.6 and V3.2 share a Unity
/// generation, which is why the manifest is preferred.
fn detect_game_line(app_path: &Path, manifest: &Manifest) -> GameLine {
    match (&manifest.installed_branch, &manifest.requested_branch) {
        (Some(installed), Some(requested)) if installed != requested => return GameLine::Updating,
        (Some(installed), _) => return line_from_beta_key(Some(installed)),
        (None, _) => {}
    }

    // No MountedConfig — fall back to the app bundle.
    let plist = app_path.join("Contents/Info.plist");
    match fs::read_to_string(plist)
        .ok()
        .and_then(|text| unity_says_alpha20(&text))
    {
        Some(true) => GameLine::A20,
        Some(false) => line_from_beta_key(manifest.requested_branch.as_deref()),
        None => GameLine::Unknown,
    }
}

/// Locate the Doorstop dylib and identify which generation it belongs to.
///
/// Returns the newest generation present, since that is the one whose
/// environment variables we must speak when launching.
pub fn find_doorstop(game_path: &Path) -> Option<(Doorstop, PathBuf)> {
    let libs = game_path.join("doorstop_libs");
    for generation in Doorstop::all() {
        for name in generation.dylib_names() {
            let candidate = libs.join(name);
            if candidate.is_file() {
                return Some((generation, candidate));
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const A20_PLIST: &str = "Unity Player version 2020.3.14f1 (d0d1bb862f9d).";
    const V2_PLIST: &str = "Unity Player version 2022.3.29f1 (8d91ffdec66b).";

    fn manifest(installed: Option<&str>, requested: Option<&str>) -> Manifest {
        Manifest {
            installed_branch: installed.map(str::to_string),
            requested_branch: requested.map(str::to_string),
            name: None,
            size_on_disk: None,
        }
    }

    /// A throwaway game folder, optionally containing an `Info.plist`.
    ///
    /// The directory name must be unique per call. Naming it after the plist
    /// (or its length) collides: the two plist constants are the same length,
    /// and separate tests reuse the same content — so parallel tests would
    /// share a directory and `remove_dir_all` one another's fixture mid-run.
    fn game_with(plist: Option<&str>) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ul-line-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        let app = dir.join("7DaysToDie.app/Contents");
        fs::create_dir_all(&app).unwrap();
        if let Some(text) = plist {
            fs::write(app.join("Info.plist"), text).unwrap();
        }
        dir.join("7DaysToDie.app")
    }

    #[test]
    fn mounted_branch_decides_when_it_matches_the_request() {
        let g = game_with(Some(V2_PLIST));
        assert_eq!(
            detect_game_line(&g, &manifest(Some("alpha20.7"), Some("alpha20.7"))),
            GameLine::A20
        );
        assert_eq!(
            detect_game_line(&g, &manifest(Some("v2.6"), Some("v2.6"))),
            GameLine::V26
        );
    }

    /// Observed live: UserConfig said `v2.6` while MountedConfig still said
    /// `alpha20.7`, because the 15 GB branch download had not finished. Steam
    /// records both, so the mismatch is directly readable.
    #[test]
    fn requested_branch_differing_from_mounted_is_updating() {
        let g = game_with(Some(A20_PLIST));
        assert_eq!(
            detect_game_line(&g, &manifest(Some("alpha20.7"), Some("v2.6"))),
            GameLine::Updating
        );
        assert_eq!(
            detect_game_line(&g, &manifest(Some("v2.6"), Some("v3.2.0"))),
            GameLine::Updating
        );
    }

    #[test]
    fn parses_betakey_from_the_right_section() {
        let acf = r#"
"AppState"
{
	"UserConfig"
	{
		"BetaKey"		"v2.6"
	}
	"MountedConfig"
	{
		"BetaKey"		"alpha20.7"
	}
}
"#;
        assert_eq!(
            vdf_get_in(acf, "UserConfig", "BetaKey").as_deref(),
            Some("v2.6")
        );
        assert_eq!(
            vdf_get_in(acf, "MountedConfig", "BetaKey").as_deref(),
            Some("alpha20.7")
        );
    }

    /// Observed live: Steam had `BetaKey "v2.6"` while the bundle was still
    /// Unity 2020.3 (A20.7) because the branch download had not finished.
    /// Trusting the branch there installs a mod for the wrong game.
    /// With no MountedConfig at all we can still recognise an A20 install
    /// from the Unity runtime in the bundle.
    #[test]
    fn falls_back_to_unity_without_a_mounted_branch() {
        assert_eq!(
            detect_game_line(&game_with(Some(A20_PLIST)), &manifest(None, Some("v2.6"))),
            GameLine::A20
        );
        assert_eq!(
            detect_game_line(&game_with(Some(V2_PLIST)), &manifest(None, Some("v2.6"))),
            GameLine::V26
        );
    }
}
