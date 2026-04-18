use crate::utils::locks::RwLockExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;

use crate::core::queue::{TaskProgressEvent, TaskStatus};
use crate::core::storage;
use crate::utils::ffmpeg;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct WorkspaceInfo {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub adapter_id: String,
    pub adapter_config: Option<String>,
    pub auto_scan: bool,
    pub clip_output_dir: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub workspace_id: i64,
    pub name: Option<String>,
    pub auto_scan: Option<bool>,
    pub clip_output_dir: Option<String>,
    pub adapter_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub path: String,
    /// Adapter type: "bililive-recorder", "generic", etc.
    pub adapter_id: String,
    /// Optional JSON config for the adapter (e.g. SMB mount info)
    #[serde(default)]
    pub adapter_config: Option<String>,
}

/// List all workspaces
#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<WorkspaceInfo>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, name, path, adapter_id, adapter_config, auto_scan, clip_output_dir, created_at FROM workspaces ORDER BY created_at DESC"
                .to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let workspaces = rows
        .iter()
        .map(|row| WorkspaceInfo {
            id: row.try_get("", "id").unwrap_or(0),
            name: row.try_get("", "name").unwrap_or_default(),
            path: row.try_get("", "path").unwrap_or_default(),
            adapter_id: row.try_get("", "adapter_id").unwrap_or_default(),
            adapter_config: row
                .try_get::<Option<String>>("", "adapter_config")
                .unwrap_or(None),
            auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
            clip_output_dir: row
                .try_get::<Option<String>>("", "clip_output_dir")
                .unwrap_or(None),
            created_at: row.try_get("", "created_at").unwrap_or_default(),
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
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|e| format!("无法创建目录: {}", e))?;
    }
    if !path.is_dir() {
        return Err("指定路径不是一个目录".to_string());
    }

    // Insert into database
    let adapter_config_sql = match &req.adapter_config {
        Some(cfg) if !cfg.is_empty() => format!("'{}'", cfg.replace('\'', "''")),
        _ => "NULL".to_string(),
    };
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO workspaces (name, path, adapter_id, adapter_config) VALUES ('{}', '{}', '{}', {})",
            req.name.replace('\'', "''"),
            req.path.replace('\'', "''"),
            req.adapter_id.replace('\'', "''"),
            adapter_config_sql,
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
                "SELECT id, name, path, adapter_id, adapter_config, auto_scan, clip_output_dir, created_at FROM workspaces WHERE path = '{}'",
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
        adapter_config: row
            .try_get::<Option<String>>("", "adapter_config")
            .unwrap_or(None),
        auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
        clip_output_dir: row
            .try_get::<Option<String>>("", "clip_output_dir")
            .unwrap_or(None),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
    };

    // Update config.toml recent workspaces
    if let Ok(mut config) = state.config.write() {
        config.add_recent_workspace(&req.path);
        let _ = config.save(&state.config_dir);
    }

    // 媒体服务器白名单登记：允许播放工作区内的视频/音频文件
    state.media_server.allow_prefix(&workspace.path);
    if let Some(ref dir) = workspace.clip_output_dir {
        if !dir.is_empty() {
            state.media_server.allow_prefix(dir);
        }
    }

    // Start file watcher for this workspace
    if workspace.auto_scan {
        if let Err(e) = state
            .watcher
            .watch(workspace.id, std::path::Path::new(&workspace.path))
        {
            tracing::warn!(
                "Failed to start watcher for workspace {}: {}",
                workspace.id,
                e
            );
        }
    }

    tracing::info!("Workspace created: {} ({})", workspace.name, workspace.path);
    Ok(workspace)
}

/// Delete a workspace (does not delete files on disk)
#[tauri::command]
pub async fn delete_workspace(state: State<'_, AppState>, workspace_id: i64) -> Result<(), String> {
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
            &format!(
                "DELETE FROM recording_sessions WHERE workspace_id = {}",
                workspace_id
            ),
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

        // Stop file watcher
        state.watcher.unwatch(workspace_id);

        tracing::info!("Workspace deleted: id={}", workspace_id);
    }

    Ok(())
}

/// Get active workspace ID from settings_kv
#[tauri::command]
pub async fn get_active_workspace(state: State<'_, AppState>) -> Result<Option<i64>, String> {
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
///
/// 异步模式：立即返回 task_id，进度通过 `task-progress` 事件推送，可通过 `cancel_scan` 取消。
/// 扫描 + FFprobe 探针阶段不触碰数据库；全部完成后才进入数据库写入阶段，
/// 因此在探针阶段取消完全不会留下脏数据。
#[tauri::command]
pub async fn scan_workspace(state: State<'_, AppState>, workspace_id: i64) -> Result<i64, String> {
    // 预检查：工作区存在 + 目录存在
    let ws_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT path, adapter_id FROM workspaces WHERE id = {}",
                workspace_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("工作区不存在".to_string())?;

    let ws_path: String = ws_row.try_get("", "path").unwrap_or_default();
    let ws_adapter: String = ws_row.try_get("", "adapter_id").unwrap_or_default();
    let dir_buf = PathBuf::from(&ws_path);
    if !dir_buf.exists() {
        return Err(format!(
            "工作区目录不存在: {}。请在工作区设置中修改路径，或删除此工作区。",
            ws_path
        ));
    }

    // 生成 task_id（内存唯一即可，不入库）
    let task_id: i64 = chrono::Utc::now().timestamp_micros();

    tracing::info!(
        "Scan task {} submitted: {} (workspace {})",
        task_id,
        ws_path,
        workspace_id
    );

    let db = state.db.clone();
    let ffprobe_path = state.ffprobe_path.read_safe().clone();

    state
        .task_queue
        .submit(task_id, move |cancel_token, progress_tx| async move {
            scan_workspace_handler(
                task_id,
                workspace_id,
                dir_buf,
                ws_adapter,
                ffprobe_path,
                db,
                cancel_token,
                progress_tx,
            )
            .await
        })
        .await;

    Ok(task_id)
}

/// 进度 message 的结构化载荷（JSON 序列化后塞入 TaskProgressEvent.message）
#[derive(Debug, Serialize)]
struct ScanProgressPayload {
    stage: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ScanResult>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ScanResult {
    pub new_files: usize,
    pub total_files: usize,
    pub total_sessions: usize,
    pub streamers: usize,
}

fn emit_progress(
    tx: &crate::core::queue::TaskProgressSender,
    task_id: i64,
    progress: f64,
    payload: ScanProgressPayload,
) {
    let message = serde_json::to_string(&payload).unwrap_or_default();
    let _ = tx.send(TaskProgressEvent {
        task_id,
        status: TaskStatus::Processing,
        progress,
        message,
    });
}

/// 实际执行扫描。错误字符串中若包含 "Task cancelled" 会被 TaskQueue 转成 Cancelled 状态。
async fn scan_workspace_handler(
    task_id: i64,
    workspace_id: i64,
    dir_buf: PathBuf,
    adapter_id: String,
    ffprobe_path: String,
    db: crate::db::Database,
    cancel_token: tokio_util::sync::CancellationToken,
    progress_tx: crate::core::queue::TaskProgressSender,
) -> Result<(), String> {
    use futures_util::stream::StreamExt;
    use std::collections::HashMap;

    macro_rules! check_cancel {
        () => {
            if cancel_token.is_cancelled() {
                return Err("Task cancelled".to_string());
            }
        };
    }

    // Stage 1: scanning（遍历目录，CPU 轻量；通过 spawn_blocking 避免阻塞异步线程）
    emit_progress(
        &progress_tx,
        task_id,
        0.02,
        ScanProgressPayload {
            stage: "scanning",
            current: None,
            total: None,
            file: Some(dir_buf.to_string_lossy().to_string()),
            path: None,
            result: None,
        },
    );
    check_cancel!();
    let scan_dir = dir_buf.clone();
    let adapter_for_scan = if adapter_id.trim().is_empty() {
        storage::detect_adapter(&dir_buf).to_string()
    } else {
        adapter_id.clone()
    };
    let scan = tokio::task::spawn_blocking(move || {
        storage::scan_with_adapter(&scan_dir, &adapter_for_scan)
    })
    .await
    .map_err(|e| format!("扫描目录失败: {}", e))?;
    check_cancel!();

    let total_files_found = scan.files.len();
    emit_progress(
        &progress_tx,
        task_id,
        0.12,
        ScanProgressPayload {
            stage: "scanning",
            current: Some(total_files_found),
            total: Some(total_files_found),
            file: None,
            path: None,
            result: None,
        },
    );

    // Stage 3: probing（FFprobe 并发探针）
    let mut files_with_duration = scan.files.clone();
    let mut probe_cache: HashMap<PathBuf, ffmpeg::ProbeResult> = HashMap::new();

    if !ffprobe_path.is_empty() && !files_with_duration.is_empty() {
        let probe_concurrency = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 4))
            .unwrap_or(4);
        let pending: Vec<(usize, PathBuf, String)> = files_with_duration
            .iter()
            .enumerate()
            .filter(|(_, f)| f.duration_ms.is_none())
            .map(|(i, f)| (i, f.file_path.clone(), f.file_name.clone()))
            .collect();
        let pending_total = pending.len();

        if pending_total == 0 {
            // 所有文件都已有 duration_ms，直接跳过进度上报
        } else {
            let mut stream =
                futures_util::stream::iter(pending.into_iter().map(|(idx, path, file_name)| {
                    let probe_path = ffprobe_path.clone();
                    async move {
                        let p = path.clone();
                        let res =
                            tokio::task::spawn_blocking(move || ffmpeg::probe(&probe_path, &p))
                                .await
                                .unwrap_or_else(|e| {
                                    Err(format!("spawn_blocking join error: {}", e))
                                });
                        (idx, path, file_name, res)
                    }
                }))
                .buffer_unordered(probe_concurrency);

            let mut done: usize = 0;
            while let Some((idx, path, file_name, res)) = stream.next().await {
                if cancel_token.is_cancelled() {
                    return Err("Task cancelled".to_string());
                }
                if let Ok(probe) = res {
                    files_with_duration[idx].duration_ms = probe.duration_ms;
                    probe_cache.insert(path.clone(), probe);
                }
                done += 1;
                let progress = 0.15 + 0.70 * (done as f64 / pending_total as f64);
                emit_progress(
                    &progress_tx,
                    task_id,
                    progress,
                    ScanProgressPayload {
                        stage: "probing",
                        current: Some(done),
                        total: Some(pending_total),
                        file: Some(file_name),
                        path: Some(path.to_string_lossy().to_string()),
                        result: None,
                    },
                );
            }
        }
    } else if ffprobe_path.is_empty() {
        tracing::warn!(
            "ffprobe not available, skipping duration detection ({} files)",
            files_with_duration.len()
        );
    }
    check_cancel!();

    // Stage 4: grouping
    emit_progress(
        &progress_tx,
        task_id,
        0.87,
        ScanProgressPayload {
            stage: "grouping",
            current: None,
            total: Some(scan.streamer_dirs.len()),
            file: None,
            path: None,
            result: None,
        },
    );
    let sessions = storage::group_into_sessions(&files_with_duration, 3600);
    check_cancel!();

    // Stage 5: writing — 数据库写入集中在末尾，探针阶段取消不会动 DB
    let sessions_total = sessions.len();
    emit_progress(
        &progress_tx,
        task_id,
        0.90,
        ScanProgressPayload {
            stage: "writing",
            current: Some(0),
            total: Some(sessions_total),
            file: None,
            path: None,
            result: None,
        },
    );

    // 清理旧 session、解绑 videos、先将所有 video 预标记为 missing
    // 随后每个扫到的 video 在 INSERT/UPDATE 时会把 file_missing 置 0，没扫到的保持 1
    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "UPDATE videos SET session_id = NULL, file_missing = 1 WHERE workspace_id = {}",
            workspace_id
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    // 防止 FK 失败：清空任何仍指向本工作区 session 的 videos.session_id（例如跨工作区残留）
    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "UPDATE videos SET session_id = NULL \
             WHERE session_id IN (SELECT id FROM recording_sessions WHERE workspace_id = {})",
            workspace_id
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "DELETE FROM recording_sessions WHERE workspace_id = {}",
            workspace_id
        ),
    )
    .await
    .map_err(|e| format!("清理旧录制场次失败: {}", e))?;

    // Import streamers
    for sd in &scan.streamer_dirs {
        let _ = sea_orm::ConnectionTrait::execute_unprepared(
            db.conn(),
            &format!(
                "INSERT OR IGNORE INTO streamers (platform, room_id, name) VALUES ('bilibili', '{}', '{}')",
                sd.room_id,
                sd.name.replace('\'', "''"),
            ),
        )
        .await;
    }

    let mut new_files: usize = 0;

    for (sess_idx, session) in sessions.iter().enumerate() {
        let streamer_id: Option<i64> = sea_orm::ConnectionTrait::query_one(
            db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT id FROM streamers WHERE room_id = '{}'",
                    session.room_id
                ),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("", "id").ok());

        let sid_sql = streamer_id
            .map(|id| id.to_string())
            .unwrap_or("NULL".to_string());

        let started_at_sql = if session.started_at.is_empty() {
            "NULL".to_string()
        } else {
            format!("'{}'", session.started_at.replace('\'', "''"))
        };

        sea_orm::ConnectionTrait::execute_unprepared(
            db.conn(),
            &format!(
                "INSERT INTO recording_sessions (workspace_id, streamer_id, title, started_at, file_count) \
                 VALUES ({}, {}, '{}', {}, {})",
                workspace_id, sid_sql,
                session.title.replace('\'', "''"),
                started_at_sql,
                session.files.len(),
            ),
        )
        .await
        .map_err(|e| format!("写入录制场次失败: {}", e))?;

        let sess_id: Option<i64> = sea_orm::ConnectionTrait::query_one(
            db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT last_insert_rowid() as id".to_string(),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("", "id").ok());

        // 没拿到 session id 时放弃关联，避免 FK 失败
        let sess_id_sql = sess_id
            .map(|id| id.to_string())
            .unwrap_or("NULL".to_string());

        for file in &session.files {
            let fp = file.file_path.to_string_lossy();

            let existing: Option<(i64, Option<i64>)> = sea_orm::ConnectionTrait::query_one(
                db.conn(),
                sea_orm::Statement::from_string(
                    sea_orm::DatabaseBackend::Sqlite,
                    format!(
                        "SELECT id, workspace_id FROM videos WHERE file_path = '{}'",
                        fp.replace('\'', "''")
                    ),
                ),
            )
            .await
            .ok()
            .flatten()
            .map(|r| {
                (
                    r.try_get::<i64>("", "id").unwrap_or(0),
                    r.try_get::<Option<i64>>("", "workspace_id").unwrap_or(None),
                )
            });

            if let Some((video_id, existing_ws)) = existing {
                let dur_sql = file
                    .duration_ms
                    .map(|d| d.to_string())
                    .unwrap_or("NULL".to_string());
                // 若旧视频属于其它工作区（例如旧工作区未清理），迁移到当前工作区以避免 UNIQUE(file_path) 冲突
                let ws_clause = if existing_ws != Some(workspace_id) {
                    format!(", workspace_id = {}", workspace_id)
                } else {
                    String::new()
                };
                sea_orm::ConnectionTrait::execute_unprepared(
                    db.conn(),
                    &format!(
                        "UPDATE videos SET session_id = {}, streamer_id = {}, file_missing = 0{}, \
                         duration_ms = CASE WHEN duration_ms IS NULL OR duration_ms = 0 THEN {} ELSE duration_ms END \
                         WHERE id = {}",
                        sess_id_sql, sid_sql, ws_clause, dur_sql, video_id
                    ),
                )
                .await
                .map_err(|e| format!("更新视频记录失败: {}", e))?;
                if existing_ws != Some(workspace_id) {
                    new_files += 1;
                }
            } else {
                let file_size = tokio::fs::metadata(&file.file_path)
                    .await
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);

                let dur = file.duration_ms;
                let (w, h) = if let Some(cached) = probe_cache.get(&file.file_path) {
                    (cached.width, cached.height)
                } else if !ffprobe_path.is_empty() {
                    let probe_path = ffprobe_path.clone();
                    let fp = file.file_path.clone();
                    match tokio::task::spawn_blocking(move || ffmpeg::probe(&probe_path, &fp)).await
                    {
                        Ok(Ok(p)) => (p.width, p.height),
                        _ => (None, None),
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
                     workspace_id, streamer_id, session_id, stream_title, recorded_at, adapter_id, has_danmaku, file_missing) \
                     VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}, {}, {}, {}, '{}', {}, 0)",
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

                match sea_orm::ConnectionTrait::execute_unprepared(db.conn(), &sql).await {
                    Ok(_) => {
                        new_files += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to insert video {}: {}",
                            file.file_path.display(),
                            e
                        );
                    }
                }
            }
        }

        // 写入阶段进度上报
        let progress = 0.90 + 0.09 * ((sess_idx + 1) as f64 / sessions_total.max(1) as f64);
        emit_progress(
            &progress_tx,
            task_id,
            progress,
            ScanProgressPayload {
                stage: "writing",
                current: Some(sess_idx + 1),
                total: Some(sessions_total),
                file: None,
                path: None,
                result: None,
            },
        );
    }

    let total_video_count: i64 = sea_orm::ConnectionTrait::query_one(
        db.conn(),
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

    let result = ScanResult {
        new_files,
        total_files: total_video_count as usize,
        total_sessions: sessions_total,
        streamers: scan.streamer_dirs.len(),
    };

    tracing::info!(
        "Scan complete (task {}): {} new + {} total files, {} sessions",
        task_id,
        result.new_files,
        result.total_files,
        result.total_sessions
    );

    // 最终进度事件携带扫描结果，供前端展示 toast
    emit_progress(
        &progress_tx,
        task_id,
        1.0,
        ScanProgressPayload {
            stage: "writing",
            current: Some(sessions_total),
            total: Some(sessions_total),
            file: None,
            path: None,
            result: Some(result),
        },
    );

    Ok(())
}

/// 取消正在执行的扫描任务
#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>, task_id: i64) -> Result<bool, String> {
    Ok(state.task_queue.cancel(task_id).await)
}

/// Update workspace editable fields
#[tauri::command]
pub async fn update_workspace(
    state: State<'_, AppState>,
    req: UpdateWorkspaceRequest,
) -> Result<WorkspaceInfo, String> {
    // Build SET clauses dynamically
    let mut set_clauses: Vec<String> = Vec::new();

    if let Some(ref name) = req.name {
        if name.trim().is_empty() {
            return Err("工作区名称不能为空".to_string());
        }
        set_clauses.push(format!("name = '{}'", name.replace('\'', "''")));
    }

    if let Some(auto_scan) = req.auto_scan {
        set_clauses.push(format!("auto_scan = {}", auto_scan as i32));
    }

    if let Some(ref adapter_id) = req.adapter_id {
        let trimmed = adapter_id.trim();
        if trimmed.is_empty() {
            return Err("适配器类型不能为空".to_string());
        }
        if !matches!(trimmed, "bililive-recorder" | "generic") {
            return Err(format!("未知的适配器类型: {}", trimmed));
        }
        set_clauses.push(format!(
            "adapter_id = '{}'",
            trimmed.replace('\'', "''")
        ));
    }

    // clip_output_dir: Some("") or Some(path) both accepted; empty string stored as NULL
    if let Some(ref dir) = req.clip_output_dir {
        if dir.trim().is_empty() {
            set_clauses.push("clip_output_dir = NULL".to_string());
        } else {
            // Validate directory exists or can be created
            let path = std::path::Path::new(dir);
            if !path.exists() {
                tokio::fs::create_dir_all(path)
                    .await
                    .map_err(|e| format!("无法创建输出目录: {}", e))?;
            }
            if !path.is_dir() {
                return Err("指定的切片输出路径不是一个目录".to_string());
            }
            set_clauses.push(format!("clip_output_dir = '{}'", dir.replace('\'', "''")));
        }
    }

    if set_clauses.is_empty() {
        return Err("没有需要更新的字段".to_string());
    }

    set_clauses.push("updated_at = datetime('now')".to_string());

    let sql = format!(
        "UPDATE workspaces SET {} WHERE id = {}",
        set_clauses.join(", "),
        req.workspace_id
    );

    sea_orm::ConnectionTrait::execute_unprepared(state.db.conn(), &sql)
        .await
        .map_err(|e| format!("更新工作区失败: {}", e))?;

    // Fetch updated workspace
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, name, path, adapter_id, adapter_config, auto_scan, clip_output_dir, created_at FROM workspaces WHERE id = {}",
                req.workspace_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("工作区不存在".to_string())?;

    let workspace = WorkspaceInfo {
        id: row.try_get("", "id").unwrap_or(0),
        name: row.try_get("", "name").unwrap_or_default(),
        path: row.try_get("", "path").unwrap_or_default(),
        adapter_id: row.try_get("", "adapter_id").unwrap_or_default(),
        adapter_config: row
            .try_get::<Option<String>>("", "adapter_config")
            .unwrap_or(None),
        auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
        clip_output_dir: row
            .try_get::<Option<String>>("", "clip_output_dir")
            .unwrap_or(None),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
    };

    // 更新 clip_output_dir 时同步登记到媒体服务器白名单
    if let Some(ref dir) = workspace.clip_output_dir {
        if !dir.is_empty() {
            state.media_server.allow_prefix(dir);
        }
    }

    // Update watcher based on auto_scan
    if workspace.auto_scan {
        if !state.watcher.is_watching(workspace.id) {
            if let Err(e) = state
                .watcher
                .watch(workspace.id, std::path::Path::new(&workspace.path))
            {
                tracing::warn!(
                    "Failed to start watcher for workspace {}: {}",
                    workspace.id,
                    e
                );
            }
        }
    } else {
        state.watcher.unwatch(workspace.id);
    }

    tracing::info!(
        "Workspace updated: {} (id={})",
        workspace.name,
        workspace.id
    );
    Ok(workspace)
}

#[derive(Debug, Serialize)]
pub struct DiskUsageInfo {
    pub output_dir: String,
    pub dir_size_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_available_bytes: u64,
}

/// Calculate directory size recursively
fn calc_dir_size(path: &Path) -> u64 {
    if !path.is_dir() {
        return 0;
    }
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = entry.metadata();
            if let Ok(m) = meta {
                if m.is_file() {
                    total += m.len();
                } else if m.is_dir() {
                    total += calc_dir_size(&entry.path());
                }
            }
        }
    }
    total
}

/// Get disk usage info for workspace's clip output directory
#[tauri::command]
pub async fn get_disk_usage(
    state: State<'_, AppState>,
    workspace_id: i64,
) -> Result<DiskUsageInfo, String> {
    // Query workspace to get clip_output_dir and path
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT path, clip_output_dir FROM workspaces WHERE id = {}",
                workspace_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "工作区不存在".to_string())?;

    let ws_path: String = row.try_get("", "path").unwrap_or_default();
    let clip_output_dir: Option<String> = row
        .try_get::<Option<String>>("", "clip_output_dir")
        .unwrap_or(None);

    let output_dir = match clip_output_dir {
        Some(ref dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(&ws_path).join("clips"),
    };

    let output_dir_str = output_dir.to_string_lossy().to_string();
    let dir_size = calc_dir_size(&output_dir);

    // Get disk space info — use the parent that exists for queries
    let query_path = if output_dir.exists() {
        output_dir.clone()
    } else {
        // Fall back to workspace path for disk queries
        PathBuf::from(&ws_path)
    };

    let disk_total = fs2::total_space(&query_path).unwrap_or(0);
    let disk_available = fs2::available_space(&query_path).unwrap_or(0);

    Ok(DiskUsageInfo {
        output_dir: output_dir_str,
        dir_size_bytes: dir_size,
        disk_total_bytes: disk_total,
        disk_available_bytes: disk_available,
    })
}

/// Check if the workspace directory path is accessible
#[tauri::command]
pub async fn check_workspace_path(
    state: State<'_, AppState>,
    workspace_id: i64,
) -> Result<bool, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT path FROM workspaces WHERE id = {}", workspace_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("工作区不存在".to_string())?;

    let ws_path: String = row.try_get("", "path").unwrap_or_default();
    let path = Path::new(&ws_path);
    Ok(path.exists() && path.is_dir())
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
