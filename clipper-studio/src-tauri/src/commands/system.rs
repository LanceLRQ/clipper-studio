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

    let ffmpeg_path = state.ffmpeg_path.read().unwrap().clone();
    let ffprobe_path = state.ffprobe_path.read().unwrap().clone();

    let ffmpeg_version = if !ffmpeg_path.is_empty() {
        ffmpeg::get_version(&ffmpeg_path)
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
        ffmpeg_available: !ffmpeg_path.is_empty(),
        ffmpeg_version,
        ffprobe_available: !ffprobe_path.is_empty(),
        has_workspaces,
        media_server_port: state.media_server_port,
    })
}

/// 返回当前是否处于调试模式（通过 --debug 命令行参数启用）
#[tauri::command]
pub fn is_debug_mode(state: State<'_, AppState>) -> bool {
    state.debug_mode
}

/// Check FFmpeg/FFprobe availability and return status
#[tauri::command]
pub fn check_ffmpeg(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let ffmpeg_path = state.ffmpeg_path.read().unwrap().clone();
    let ffprobe_path = state.ffprobe_path.read().unwrap().clone();

    let ffmpeg_version = if !ffmpeg_path.is_empty() {
        ffmpeg::get_version(&ffmpeg_path)
    } else {
        None
    };

    Ok(serde_json::json!({
        "ffmpeg": {
            "available": !ffmpeg_path.is_empty(),
            "path": &ffmpeg_path,
            "version": ffmpeg_version,
        },
        "ffprobe": {
            "available": !ffprobe_path.is_empty(),
            "path": &ffprobe_path,
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

// ==================== Dashboard Statistics ====================

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub video_count: i64,
    pub total_duration_ms: i64,
    pub total_storage_bytes: i64,
    pub streamer_count: i64,
    pub session_count: i64,
    pub subtitle_video_count: i64,
    pub danmaku_video_count: i64,
    pub clip_total: i64,
    pub clip_completed: i64,
    pub clip_failed: i64,
    pub clip_output_bytes: i64,
    pub recent_clips: Vec<RecentClipInfo>,
    pub top_streamers: Vec<TopStreamerInfo>,
}

#[derive(Debug, Serialize)]
pub struct RecentClipInfo {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct TopStreamerInfo {
    pub name: String,
    pub video_count: i64,
    pub total_duration_ms: i64,
}

#[tauri::command]
pub async fn get_dashboard_stats(
    state: State<'_, AppState>,
    workspace_id: Option<i64>,
) -> Result<DashboardStats, String> {
    let db = state.db.conn();
    let vid_where = workspace_id
        .map(|id| format!("WHERE workspace_id = {}", id))
        .unwrap_or_default();

    // Video stats
    let video_row = sea_orm::ConnectionTrait::query_one(
        db,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) as cnt, COALESCE(SUM(duration_ms),0) as dur, COALESCE(SUM(file_size),0) as sz, \
                 COALESCE(SUM(CASE WHEN has_subtitle=1 THEN 1 ELSE 0 END),0) as sub_cnt, \
                 COALESCE(SUM(CASE WHEN has_danmaku=1 THEN 1 ELSE 0 END),0) as dm_cnt \
                 FROM videos {}",
                vid_where
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let (video_count, total_duration_ms, total_storage_bytes, subtitle_video_count, danmaku_video_count) =
        video_row
            .map(|r| {
                (
                    r.try_get::<i64>("", "cnt").unwrap_or(0),
                    r.try_get::<i64>("", "dur").unwrap_or(0),
                    r.try_get::<i64>("", "sz").unwrap_or(0),
                    r.try_get::<i64>("", "sub_cnt").unwrap_or(0),
                    r.try_get::<i64>("", "dm_cnt").unwrap_or(0),
                )
            })
            .unwrap_or((0, 0, 0, 0, 0));

    // Streamer count: only streamers that have videos in this workspace
    let streamer_sql = match workspace_id {
        Some(ws_id) => format!(
            "SELECT COUNT(DISTINCT streamer_id) as cnt FROM videos \
             WHERE workspace_id = {} AND streamer_id IS NOT NULL",
            ws_id
        ),
        None => "SELECT COUNT(*) as cnt FROM streamers".to_string(),
    };
    let streamer_count = sea_orm::ConnectionTrait::query_one(
        db,
        sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, streamer_sql),
    )
    .await
    .map_err(|e| e.to_string())?
    .map(|r| r.try_get::<i64>("", "cnt").unwrap_or(0))
    .unwrap_or(0);

    // Session count
    let sess_where = workspace_id
        .map(|id| format!("WHERE workspace_id = {}", id))
        .unwrap_or_default();
    let session_count = sea_orm::ConnectionTrait::query_one(
        db,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT COUNT(*) as cnt FROM recording_sessions {}", sess_where),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .map(|r| r.try_get::<i64>("", "cnt").unwrap_or(0))
    .unwrap_or(0);

    // Clip stats (JOIN videos for workspace filter)
    let clip_join = workspace_id
        .map(|id| format!(
            "INNER JOIN videos v ON t.video_id = v.id WHERE v.workspace_id = {}", id
        ))
        .unwrap_or_default();
    let clip_row = sea_orm::ConnectionTrait::query_one(
        db,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) as total, \
                 COALESCE(SUM(CASE WHEN t.status='completed' THEN 1 ELSE 0 END),0) as done, \
                 COALESCE(SUM(CASE WHEN t.status='failed' THEN 1 ELSE 0 END),0) as fail \
                 FROM clip_tasks t {}",
                clip_join
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let (clip_total, clip_completed, clip_failed) = clip_row
        .map(|r| {
            (
                r.try_get::<i64>("", "total").unwrap_or(0),
                r.try_get::<i64>("", "done").unwrap_or(0),
                r.try_get::<i64>("", "fail").unwrap_or(0),
            )
        })
        .unwrap_or((0, 0, 0));

    let clip_out_join = workspace_id
        .map(|id| format!(
            "INNER JOIN videos v ON co.video_id = v.id WHERE v.workspace_id = {}", id
        ))
        .unwrap_or_default();
    let clip_output_bytes = sea_orm::ConnectionTrait::query_one(
        db,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT COALESCE(SUM(co.file_size),0) as sz FROM clip_outputs co {}", clip_out_join),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .map(|r| r.try_get::<i64>("", "sz").unwrap_or(0))
    .unwrap_or(0);

    // Recent clips (last 10)
    let recent_join = workspace_id
        .map(|id| format!(
            "INNER JOIN videos v ON t.video_id = v.id WHERE v.workspace_id = {}", id
        ))
        .unwrap_or_default();
    let recent_rows = sea_orm::ConnectionTrait::query_all(
        db,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.id, COALESCE(t.title,'') as title, t.status, t.created_at \
                 FROM clip_tasks t {} ORDER BY t.created_at DESC LIMIT 10",
                recent_join
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let recent_clips: Vec<RecentClipInfo> = recent_rows
        .iter()
        .map(|r| RecentClipInfo {
            id: r.try_get("", "id").unwrap_or(0),
            title: r.try_get("", "title").unwrap_or_default(),
            status: r.try_get("", "status").unwrap_or_default(),
            created_at: r.try_get("", "created_at").unwrap_or_default(),
        })
        .collect();

    // Top streamers (by video count, top 5)
    let top_vid_where = workspace_id
        .map(|id| format!("WHERE v.workspace_id = {}", id))
        .unwrap_or_default();
    let top_rows = sea_orm::ConnectionTrait::query_all(
        db,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT st.name, COUNT(v.id) as vcnt, COALESCE(SUM(v.duration_ms),0) as dur \
                 FROM streamers st INNER JOIN videos v ON st.id = v.streamer_id \
                 {} GROUP BY st.id ORDER BY vcnt DESC LIMIT 5",
                top_vid_where
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let top_streamers: Vec<TopStreamerInfo> = top_rows
        .iter()
        .map(|r| TopStreamerInfo {
            name: r.try_get("", "name").unwrap_or_default(),
            video_count: r.try_get("", "vcnt").unwrap_or(0),
            total_duration_ms: r.try_get("", "dur").unwrap_or(0),
        })
        .collect();

    Ok(DashboardStats {
        video_count,
        total_duration_ms,
        total_storage_bytes,
        streamer_count,
        session_count,
        subtitle_video_count,
        danmaku_video_count,
        clip_total,
        clip_completed,
        clip_failed,
        clip_output_bytes,
        recent_clips,
        top_streamers,
    })
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

/// Force close the application window (called after user confirms close dialog)
#[tauri::command]
pub fn confirm_close(window: tauri::Window) -> Result<(), String> {
    window.destroy().map_err(|e| e.to_string())
}
