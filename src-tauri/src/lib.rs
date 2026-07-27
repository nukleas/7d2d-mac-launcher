mod paths;
mod progress;
mod steam;
mod ul;

use paths::expand_user_path;
use serde::Serialize;
use steam::{detect_game, free_space_bytes, GameInfo};
use tauri::AppHandle;
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

/// Lightweight reads — still offload so the webview never stalls on disk I/O.
#[tauri::command]
async fn get_game_info(path: Option<String>) -> GameInfo {
    tauri::async_runtime::spawn_blocking(move || detect_game(path))
        .await
        .unwrap_or_else(|e| GameInfo {
            found: false,
            game_path: String::new(),
            app_path: String::new(),
            manifest_path: String::new(),
            beta_key: None,
            name: None,
            size_on_disk_bytes: None,
            looks_like_a20: false,
            has_bepinex: false,
            has_run_bepinex: false,
            has_doorstop: false,
            has_mods_folder: false,
            mod_ready: false,
            notes: vec![format!("Background task failed: {e}")],
        })
}

#[tauri::command]
async fn get_disk_info(path: Option<String>) -> DiskInfo {
    tauri::async_runtime::spawn_blocking(move || {
        let p = path
            .map(|s| expand_user_path(&s))
            .unwrap_or_else(|| expand_user_path("~/Library/Application Support/Steam"));
        let free = free_space_bytes(&p);
        DiskInfo {
            path: p.to_string_lossy().into_owned(),
            free_bytes: free,
            free_gb: free.map(|b| b as f64 / 1_073_741_824.0),
        }
    })
    .await
    .unwrap_or(DiskInfo {
        path: String::new(),
        free_bytes: None,
        free_gb: None,
    })
}

/// Heavy install (download / unzip / copy) always runs on a blocking worker pool
/// so the UI thread stays responsive and progress events can paint.
#[tauri::command]
async fn install_ul_experimental(
    app: AppHandle,
    path: Option<String>,
    force: Option<bool>,
) -> InstallResult {
    let force = force.unwrap_or(false);
    let app_for_job = app.clone();

    match tauri::async_runtime::spawn_blocking(move || {
        install_undead_legacy_experimental(&app_for_job, path, force)
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            crate::progress::progress(
                &app,
                "error",
                "Install crashed",
                format!("Background worker failed: {e}"),
                0,
            );
            InstallResult {
                ok: false,
                message: format!(
                    "The install worker stopped unexpectedly. Try again. ({e})"
                ),
                game_path: String::new(),
                steps: vec![],
                download_bytes: None,
            }
        }
    }
}

#[tauri::command]
async fn launch_ul(path: Option<String>) -> LaunchResult {
    tauri::async_runtime::spawn_blocking(move || launch_undead_legacy(path))
        .await
        .unwrap_or_else(|e| LaunchResult {
            ok: false,
            message: format!("Could not start launch worker: {e}"),
            command: String::new(),
        })
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
