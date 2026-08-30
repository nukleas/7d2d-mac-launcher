//! Progress events for the frontend (friendly stages + percentages).

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    /// Machine stage id: check | download | extract | copy | finish | error
    pub stage: String,
    /// Human-friendly one-liner for the main UI
    pub title: String,
    /// Extra detail (path, bytes, file name)
    pub detail: String,
    /// 0–100
    pub percent: u8,
    pub bytes_done: Option<u64>,
    pub bytes_total: Option<u64>,
    pub indeterminate: bool,
}

/// Where install progress goes.
///
/// The install pipeline is long-running and worth testing end to end, which it
/// cannot be if every step needs a live Tauri app. A silent sink lets the real
/// download/unpack/install run headless.
#[derive(Clone, Default)]
pub struct Progress {
    app: Option<AppHandle>,
}

impl Progress {
    pub fn to_app(app: AppHandle) -> Self {
        Self { app: Some(app) }
    }

    /// Discards events, so the install pipeline can run headless under test.
    #[cfg(test)]
    pub fn silent() -> Self {
        Self { app: None }
    }

    pub fn emit(&self, event: ProgressEvent) {
        if let Some(app) = &self.app {
            let _ = app.emit("install-progress", event);
        }
    }
}

pub fn progress(
    app: &Progress,
    stage: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    percent: u8,
) {
    app.emit(ProgressEvent {
        stage: stage.into(),
        title: title.into(),
        detail: detail.into(),
        percent: percent.min(100),
        bytes_done: None,
        bytes_total: None,
        indeterminate: false,
    });
}

pub fn progress_bytes(
    app: &Progress,
    stage: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    percent: u8,
    done: u64,
    total: Option<u64>,
) {
    app.emit(ProgressEvent {
        stage: stage.into(),
        title: title.into(),
        detail: detail.into(),
        percent: percent.min(100),
        bytes_done: Some(done),
        bytes_total: total,
        indeterminate: total.is_none(),
    });
}
