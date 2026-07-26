mod paths;
mod steam;
mod ul;

use paths::expand_user_path;
use serde::Serialize;
use steam::{detect_game, free_space_bytes, GameInfo};
use ul::{install_undead_legacy_experimental, launch_undead_legacy, InstallResult, LaunchResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskInfo {
    path: String,
    free_bytes: Option<u64>,
    free_gb: Option<f64>,
}

#[tauri::command]
fn expand_path(path: String) -> String {
    expand_user_path(&path).to_string_lossy().into_owned()
}

#[tauri::command]
fn get_game_info(path: Option<String>) -> GameInfo {
    detect_game(path)
}

#[tauri::command]
fn get_disk_info(path: Option<String>) -> DiskInfo {
    let p = path
        .map(|s| expand_user_path(&s))
        .unwrap_or_else(|| expand_user_path("~/Library/Application Support/Steam"));
    let free = free_space_bytes(&p);
    DiskInfo {
        path: p.to_string_lossy().into_owned(),
        free_bytes: free,
        free_gb: free.map(|b| b as f64 / 1_073_741_824.0),
    }
}

#[tauri::command]
fn install_ul_experimental(path: Option<String>, force: Option<bool>) -> InstallResult {
    install_undead_legacy_experimental(path, force.unwrap_or(false))
}

#[tauri::command]
fn launch_ul(path: Option<String>) -> LaunchResult {
    launch_undead_legacy(path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            expand_path,
            get_game_info,
            get_disk_info,
            install_ul_experimental,
            launch_ul
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
