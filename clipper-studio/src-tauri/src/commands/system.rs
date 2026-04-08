use serde::Serialize;
use tauri::{Manager, State};
use std::path::PathBuf;

use crate::AppState;
use crate::utils::ffmpeg;

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub config_path: String,
    pub ffmpeg_available: bool,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_available: bool,
    pub has_workspaces: bool,
    pub media_server_port: u16,
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

    let config_path = state.config_dir.join("config.toml").to_string_lossy().to_string();

    // Check if any workspaces exist (for welcome wizard logic)
    let has_workspaces = tauri::async_runtime::block_on(async {
        let result = sea_orm::ConnectionTrait::query_one(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT COUNT(*) as cnt FROM workspaces".to_string(),
            ),
        )
        .await;
        match result {
            Ok(Some(row)) => {
                
                let cnt: i32 = row.try_get("", "cnt").unwrap_or(0);
                cnt > 0
            }
            _ => false,
        }
    });

    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir,
        config_path,
        ffmpeg_available: !state.ffmpeg_path.is_empty(),
        ffmpeg_version,
        ffprobe_available: !state.ffprobe_path.is_empty(),
        has_workspaces,
        media_server_port: state.media_server_port,
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

/// Track a local analytics event
#[tauri::command]
pub async fn track_event(
    state: State<'_, AppState>,
    event: String,
    properties: Option<serde_json::Value>,
) -> Result<(), String> {
    let props_sql = properties
        .map(|p| format!("'{}'", p.to_string().replace('\'', "''")))
        .unwrap_or("NULL".to_string());

    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO analytics_events (event, properties) VALUES ('{}', {})",
            event.replace('\'', "''"),
            props_sql
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get a setting value from settings_kv
#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT value FROM settings_kv WHERE key = '{}'",
                key.replace('\'', "''")
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(row.and_then(|r| r.try_get::<String>("", "value").ok()))
}

/// Set a setting value in settings_kv
#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('{}', '{}')",
            key.replace('\'', "''"),
            value.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get multiple settings at once (batch read)
#[tauri::command]
pub async fn get_settings(
    state: State<'_, AppState>,
    keys: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut result = std::collections::HashMap::new();
    for key in &keys {
        let row = sea_orm::ConnectionTrait::query_one(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT value FROM settings_kv WHERE key = '{}'",
                    key.replace('\'', "''")
                ),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        if let Some(row) = row {
            if let Ok(val) = row.try_get::<String>("", "value") {
                result.insert(key.clone(), val);
            }
        }
    }
    Ok(result)
}

/// Reveal a file in the system file manager (Finder/Explorer)
#[tauri::command]
pub fn reveal_file(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("文件不存在".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        // Open parent directory on Linux
        if let Some(parent) = p.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Open a file with the system default application
#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("文件不存在".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
