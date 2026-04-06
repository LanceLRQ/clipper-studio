use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::State;

use crate::AppState;
use crate::core::storage;
use crate::utils::ffmpeg;

#[derive(Debug, Serialize)]
pub struct WorkspaceInfo {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub adapter_id: String,
    pub auto_scan: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub path: String,
    /// Adapter type: "bililive-recorder", "generic", etc.
    pub adapter_id: String,
}

/// List all workspaces
#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, AppState>,
) -> Result<Vec<WorkspaceInfo>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, name, path, adapter_id, auto_scan, created_at FROM workspaces ORDER BY created_at DESC"
                .to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let workspaces = rows
        .iter()
        .map(|row| {
            
            WorkspaceInfo {
                id: row.try_get("", "id").unwrap_or(0),
                name: row.try_get("", "name").unwrap_or_default(),
                path: row.try_get("", "path").unwrap_or_default(),
                adapter_id: row.try_get("", "adapter_id").unwrap_or_default(),
                auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
                created_at: row.try_get("", "created_at").unwrap_or_default(),
            }
        })
        .collect();

    Ok(workspaces)
}

/// Create a new workspace
#[tauri::command]
pub async fn create_workspace(
    state: State<'_, AppState>,
    req: CreateWorkspaceRequest,
) -> Result<WorkspaceInfo, String> {
    // Validate path exists
    let path = std::path::Path::new(&req.path);
    if !path.exists() {
        // Try to create the directory for new workspaces
        std::fs::create_dir_all(path).map_err(|e| format!("无法创建目录: {}", e))?;
    }
    if !path.is_dir() {
        return Err("指定路径不是一个目录".to_string());
    }

    // Insert into database
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO workspaces (name, path, adapter_id) VALUES ('{}', '{}', '{}')",
            req.name.replace('\'', "''"),
            req.path.replace('\'', "''"),
            req.adapter_id.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| format!("创建工作区失败: {}", e))?;

    // Get the created workspace
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, name, path, adapter_id, auto_scan, created_at FROM workspaces WHERE path = '{}'",
                req.path.replace('\'', "''")
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("创建后查询失败".to_string())?;

    
    let workspace = WorkspaceInfo {
        id: row.try_get("", "id").unwrap_or(0),
        name: row.try_get("", "name").unwrap_or_default(),
        path: row.try_get("", "path").unwrap_or_default(),
        adapter_id: row.try_get("", "adapter_id").unwrap_or_default(),
        auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
    };

    // Update config.toml recent workspaces
    if let Ok(mut config) = state.config.write() {
        config.add_recent_workspace(&req.path);
        let _ = config.save(&state.config_dir);
    }

    tracing::info!("Workspace created: {} ({})", workspace.name, workspace.path);
    Ok(workspace)
}

/// Delete a workspace (does not delete files on disk)
#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, AppState>,
    workspace_id: i64,
) -> Result<(), String> {
    // Get workspace path before deletion (for config cleanup)
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT path FROM workspaces WHERE id = {}", workspace_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    if let Some(row) = row {
        
        let path: String = row.try_get("", "path").unwrap_or_default();

        // Remove related data
        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!("DELETE FROM videos WHERE workspace_id = {}", workspace_id),
        )
        .await
        .map_err(|e| e.to_string())?;

        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!("DELETE FROM recording_sessions WHERE workspace_id = {}", workspace_id),
        )
        .await
        .map_err(|e| e.to_string())?;

        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!("DELETE FROM workspaces WHERE id = {}", workspace_id),
        )
        .await
        .map_err(|e| e.to_string())?;

        // Update config.toml
        if let Ok(mut config) = state.config.write() {
            config.remove_recent_workspace(&path);
            let _ = config.save(&state.config_dir);
        }

        tracing::info!("Workspace deleted: id={}", workspace_id);
    }

    Ok(())
}

/// Get active workspace ID from settings_kv
#[tauri::command]
pub async fn get_active_workspace(
    state: State<'_, AppState>,
) -> Result<Option<i64>, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'active_workspace_id'".to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    match row {
        Some(row) => {
            
            let value: String = row.try_get("", "value").unwrap_or_default();
            Ok(value.parse::<i64>().ok())
        }
        None => Ok(None),
    }
}

/// Set active workspace ID
#[tauri::command]
pub async fn set_active_workspace(
    state: State<'_, AppState>,
    workspace_id: Option<i64>,
) -> Result<(), String> {
    match workspace_id {
        Some(id) => {
            sea_orm::ConnectionTrait::execute_unprepared(
                state.db.conn(),
                &format!(
                    "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('active_workspace_id', '{}')",
                    id
                ),
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        None => {
            sea_orm::ConnectionTrait::execute_unprepared(
                state.db.conn(),
                "DELETE FROM settings_kv WHERE key = 'active_workspace_id'",
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Scan a workspace directory and import all found videos into the database.
/// Auto-detects adapter (BililiveRecorder, generic).
#[tauri::command]
pub async fn scan_workspace(
    state: State<'_, AppState>,
    workspace_id: i64,
) -> Result<ScanResult, String> {
    // Get workspace path
    let ws_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT path FROM workspaces WHERE id = {}", workspace_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("工作区不存在".to_string())?;

    let ws_path: String = ws_row.try_get("", "path").unwrap_or_default();
    let dir = Path::new(&ws_path);

    if !dir.exists() {
        return Err(format!("工作区目录不存在: {}", ws_path));
    }

    tracing::info!("Scanning workspace: {} (id={})", ws_path, workspace_id);

    // Step 1: Clear old sessions and detach videos (SET NULL)
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "UPDATE videos SET session_id = NULL WHERE workspace_id = {}",
            workspace_id
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "DELETE FROM recording_sessions WHERE workspace_id = {}",
            workspace_id
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Step 2: Scan directory
    let scan = storage::scan_workspace(dir);

    // Import streamers
    for sd in &scan.streamer_dirs {
        let _ = sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!(
                "INSERT OR IGNORE INTO streamers (platform, room_id, name) VALUES ('bilibili', '{}', '{}')",
                sd.room_id,
                sd.name.replace('\'', "''"),
            ),
        )
        .await;
    }

    // Step 3: Fill duration_ms via FFprobe before grouping (needed for accurate session merging)
    let mut files_with_duration = scan.files.clone();
    if !state.ffprobe_path.is_empty() {
        for file in &mut files_with_duration {
            if file.duration_ms.is_none() {
                if let Ok(probe) = ffmpeg::probe(&state.ffprobe_path, &file.file_path) {
                    file.duration_ms = probe.duration_ms;
                }
            }
        }
    }

    // Step 4: Group into sessions (gap = next.start - prev.end > 1 hour)
    let sessions = storage::group_into_sessions(&files_with_duration, 3600);

    let mut new_files = 0;

    for session in &sessions {
        // Get streamer_id
        let streamer_id: Option<i64> = sea_orm::ConnectionTrait::query_one(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!("SELECT id FROM streamers WHERE room_id = '{}'", session.room_id),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("", "id").ok());

        let sid_sql = streamer_id.map(|id| id.to_string()).unwrap_or("NULL".to_string());

        // Insert session
        let _ = sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!(
                "INSERT INTO recording_sessions (workspace_id, streamer_id, title, started_at, file_count) \
                 VALUES ({}, {}, '{}', '{}', {})",
                workspace_id, sid_sql,
                session.title.replace('\'', "''"),
                session.started_at,
                session.files.len(),
            ),
        )
        .await;

        let sess_id: Option<i64> = sea_orm::ConnectionTrait::query_one(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT last_insert_rowid() as id".to_string(),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("", "id").ok());

        let sess_id_sql = sess_id.map(|id| id.to_string()).unwrap_or("NULL".to_string());

        for file in &session.files {
            let fp = file.file_path.to_string_lossy();

            // Check if video already exists in DB
            let existing: Option<i64> = sea_orm::ConnectionTrait::query_one(
                state.db.conn(),
                sea_orm::Statement::from_string(
                    sea_orm::DatabaseBackend::Sqlite,
                    format!(
                        "SELECT id FROM videos WHERE file_path = '{}'",
                        fp.replace('\'', "''")
                    ),
                ),
            )
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get("", "id").ok());

            if let Some(video_id) = existing {
                // Video exists: just re-attach to new session
                let _ = sea_orm::ConnectionTrait::execute_unprepared(
                    state.db.conn(),
                    &format!(
                        "UPDATE videos SET session_id = {}, streamer_id = {} WHERE id = {}",
                        sess_id_sql, sid_sql, video_id
                    ),
                )
                .await;
            } else {
                // New video: probe and insert
                let file_size = std::fs::metadata(&file.file_path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);

                let dur = file.duration_ms;
                let (w, h) = if !state.ffprobe_path.is_empty() {
                    match ffmpeg::probe(&state.ffprobe_path, &file.file_path) {
                        Ok(p) => (p.width, p.height),
                        Err(_) => (None, None),
                    }
                } else {
                    (None, None)
                };

                let has_danmaku = file
                    .associated_files
                    .iter()
                    .any(|p| p.extension().and_then(|e| e.to_str()) == Some("xml"));

                let sql = format!(
                    "INSERT INTO videos (file_path, file_name, file_size, duration_ms, width, height, \
                     workspace_id, streamer_id, session_id, stream_title, recorded_at, adapter_id, has_danmaku) \
                     VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}, {}, {}, {}, '{}', {})",
                    fp.replace('\'', "''"),
                    file.file_name.replace('\'', "''"),
                    file_size,
                    dur.map(|d| d.to_string()).unwrap_or("NULL".to_string()),
                    w.map(|v| v.to_string()).unwrap_or("NULL".to_string()),
                    h.map(|v| v.to_string()).unwrap_or("NULL".to_string()),
                    workspace_id,
                    sid_sql,
                    sess_id_sql,
                    file.stream_title.as_deref().map(|t| format!("'{}'", t.replace('\'', "''"))).unwrap_or("NULL".to_string()),
                    file.recorded_at.as_deref().map(|t| format!("'{}'", t)).unwrap_or("NULL".to_string()),
                    scan.adapter_id,
                    has_danmaku as i32,
                );

                if sea_orm::ConnectionTrait::execute_unprepared(state.db.conn(), &sql).await.is_ok()
                {
                    new_files += 1;
                }
            }
        }
    }

    let total_video_count: i64 = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) as cnt FROM videos WHERE workspace_id = {}",
                workspace_id
            ),
        ),
    )
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get("", "cnt").ok())
    .unwrap_or(0);

    tracing::info!(
        "Scan complete: {} new + {} total files, {} sessions",
        new_files,
        total_video_count,
        sessions.len()
    );

    Ok(ScanResult {
        new_files,
        total_files: total_video_count as usize,
        total_sessions: sessions.len(),
        streamers: scan.streamer_dirs.len(),
    })
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub new_files: usize,
    pub total_files: usize,
    pub total_sessions: usize,
    pub streamers: usize,
}

/// Detect adapter type for a given directory path
#[tauri::command]
pub fn detect_workspace_adapter(path: String) -> Result<String, String> {
    let dir = Path::new(&path);
    if !dir.exists() {
        return Err("目录不存在".to_string());
    }
    Ok(storage::detect_adapter(dir).to_string())
}
