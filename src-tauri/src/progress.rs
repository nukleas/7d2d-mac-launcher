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

pub fn emit_progress(app: &AppHandle, event: ProgressEvent) {
    let _ = app.emit("install-progress", event);
}

pub fn progress(
    app: &AppHandle,
    stage: &str,
    title: &str,
    detail: impl Into<String>,
    percent: u8,
) {
    emit_progress(
        app,
        ProgressEvent {
            stage: stage.into(),
            title: title.into(),
            detail: detail.into(),
            percent: percent.min(100),
            bytes_done: None,
            bytes_total: None,
            indeterminate: false,
        },
    );
}

pub fn progress_bytes(
    app: &AppHandle,
    stage: &str,
    title: &str,
    detail: impl Into<String>,
    percent: u8,
    done: u64,
    total: Option<u64>,
) {
    emit_progress(
        app,
        ProgressEvent {
            stage: stage.into(),
            title: title.into(),
            detail: detail.into(),
            percent: percent.min(100),
            bytes_done: Some(done),
            bytes_total: total,
            indeterminate: total.is_none(),
        },
    );
}
