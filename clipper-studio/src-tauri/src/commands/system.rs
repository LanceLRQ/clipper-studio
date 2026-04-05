use serde::Serialize;
use tauri::{Manager, State};
use std::path::PathBuf;

use crate::AppState;
use crate::utils::ffmpeg;

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub ffmpeg_available: bool,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_available: bool,
}

/// Get application info including version, data directory, and FFmpeg availability
#[tauri::command]
pub fn get_app_info(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AppInfo, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map(|p: PathBuf| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let ffmpeg_version = if !state.ffmpeg_path.is_empty() {
        ffmpeg::get_version(&state.ffmpeg_path)
    } else {
        None
    };

    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir,
        ffmpeg_available: !state.ffmpeg_path.is_empty(),
        ffmpeg_version,
        ffprobe_available: !state.ffprobe_path.is_empty(),
    })
}

/// Check FFmpeg/FFprobe availability and return status
#[tauri::command]
pub fn check_ffmpeg(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let ffmpeg_version = if !state.ffmpeg_path.is_empty() {
        ffmpeg::get_version(&state.ffmpeg_path)
    } else {
        None
    };

    Ok(serde_json::json!({
        "ffmpeg": {
            "available": !state.ffmpeg_path.is_empty(),
            "path": &state.ffmpeg_path,
            "version": ffmpeg_version,
        },
        "ffprobe": {
            "available": !state.ffprobe_path.is_empty(),
            "path": &state.ffprobe_path,
        }
    }))
}
