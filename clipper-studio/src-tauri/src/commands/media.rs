use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{Emitter, State};

use crate::AppState;
use crate::core::clipper::{self, PresetOptions};
use crate::core::queue::{TaskProgressEvent, TaskStatus};

// ====== Types ======

#[derive(Debug, Serialize)]
pub struct MediaTaskInfo {
    pub id: i64,
    pub task_type: String,
    pub video_ids: Vec<i64>,
    pub output_path: Option<String>,
    pub status: String,
    pub progress: f64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

// ====== Transcode ======

#[derive(Debug, Deserialize)]
pub struct TranscodeRequest {
    pub video_id: i64,
    pub preset_id: i64,
    pub output_dir: Option<String>,
}

/// Transcode a video with a specified encoding preset
#[tauri::command]
pub async fn transcode_video(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    req: TranscodeRequest,
) -> Result<MediaTaskInfo, String> {
    // Get video info
    let video_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT file_path, file_name, duration_ms FROM videos WHERE id = {}",
                req.video_id,
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let file_path: String = video_row.try_get("", "file_path").unwrap_or_default();
    let file_name: String = video_row.try_get("", "file_name").unwrap_or_default();
    let duration_ms: i64 = video_row.try_get("", "duration_ms").unwrap_or(0);

    // Load preset
    let preset = load_preset(state.db.conn(), req.preset_id).await?;

    // Determine output path
    let ext = if preset.audio_only.unwrap_or(false) { "m4a" } else { "mp4" };
    let output_dir = match &req.output_dir {
        Some(dir) => PathBuf::from(dir),
        None => {
            let src = PathBuf::from(&file_path);
            src.parent().unwrap_or(&PathBuf::from(".")).to_path_buf()
        }
    };

    let stem = file_name.rsplit('.').last().unwrap_or(&file_name);
    let output_filename = format!("{}_transcoded.{}", stem, ext);
    let output_path = output_dir.join(&output_filename);

    // Insert media_tasks record
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO media_tasks (task_type, video_ids, output_path, preset_id, status) \
             VALUES ('transcode', '[{}]', '{}', {}, 'pending')",
            req.video_id,
            output_path.to_string_lossy().replace('\'', "''"),
            req.preset_id,
        ),
    )
    .await
    .map_err(|e| format!("创建转码任务失败: {}", e))?;

    let task_id = get_last_insert_id(state.db.conn()).await?;

    // Submit to task queue
    let ffmpeg_path = state.ffmpeg_path.read().unwrap().clone();
    let db = state.db.clone();
    let input = PathBuf::from(file_path);
    let output = output_path.clone();
    let app = app_handle.clone();

    state
        .task_queue
        .submit(task_id, move |cancel_token, progress_tx| async move {
            let _ = update_media_task_status(&db, task_id, "processing", None).await;

            let progress_tx_clone = progress_tx.clone();
            let result = clipper::execute_clip(
                &ffmpeg_path,
                &input,
                &output,
                0,
                duration_ms,
                &preset,
                cancel_token,
                move |p| {
                    let _ = progress_tx_clone.send(TaskProgressEvent {
                        task_id,
                        status: TaskStatus::Processing,
                        progress: p.progress,
                        message: format!(
                            "转码 {:.0}% (速度: {:.1}x)",
                            p.progress * 100.0,
                            p.speed.unwrap_or(0.0)
                        ),
                    });
                },
            )
            .await;

            match &result {
                Ok(()) => {
                    let file_size = std::fs::metadata(&output)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);
                    let _ = update_media_task_status(&db, task_id, "completed", None).await;
                    let _ = app.emit("media-task-completed", serde_json::json!({
                        "task_id": task_id,
                        "task_type": "transcode",
                        "output_path": output.to_string_lossy(),
                        "file_size": file_size,
                    }));
                }
                Err(e) => {
                    let status = if e == "Task cancelled" { "cancelled" } else { "failed" };
                    let _ = update_media_task_status(&db, task_id, status, Some(e)).await;
                }
            }

            result
        })
        .await;

    Ok(MediaTaskInfo {
        id: task_id,
        task_type: "transcode".to_string(),
        video_ids: vec![req.video_id],
        output_path: Some(output_path.to_string_lossy().to_string()),
        status: "pending".to_string(),
        progress: 0.0,
        error_message: None,
        created_at: String::new(),
        completed_at: None,
    })
}

/// List media tasks (transcode, merge)
#[tauri::command]
pub async fn list_media_tasks(
    state: State<'_, AppState>,
    task_type: Option<String>,
) -> Result<Vec<MediaTaskInfo>, String> {
    let where_clause = match &task_type {
        Some(t) => format!("WHERE task_type = '{}'", t.replace('\'', "''")),
        None => String::new(),
    };

    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM media_tasks {} ORDER BY created_at DESC",
                where_clause,
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let tasks = rows
        .iter()
        .map(|row| {
            let video_ids_json: String = row.try_get("", "video_ids").unwrap_or_default();
            let video_ids: Vec<i64> =
                serde_json::from_str(&video_ids_json).unwrap_or_default();
            MediaTaskInfo {
                id: row.try_get("", "id").unwrap_or(0),
                task_type: row.try_get("", "task_type").unwrap_or_default(),
                video_ids,
                output_path: row.try_get("", "output_path").ok(),
                status: row.try_get("", "status").unwrap_or_default(),
                progress: row.try_get("", "progress").unwrap_or(0.0),
                error_message: row.try_get("", "error_message").ok(),
                created_at: row.try_get("", "created_at").unwrap_or_default(),
                completed_at: row.try_get("", "completed_at").ok(),
            }
        })
        .collect();

    Ok(tasks)
}

// ====== Merge ======

#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub video_ids: Vec<i64>,
    /// "virtual" (stream copy) or "physical" (re-encode)
    pub mode: String,
    pub preset_id: Option<i64>,
    pub output_dir: Option<String>,
}

/// Merge multiple videos into a single file
#[tauri::command]
pub async fn merge_videos(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    req: MergeRequest,
) -> Result<MediaTaskInfo, String> {
    if req.video_ids.len() < 2 {
        return Err("至少需要选择 2 个视频".to_string());
    }

    // Get all video file paths
    let ids_str = req
        .video_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.id, v.file_path, v.file_name, w.clip_output_dir as ws_clip_output_dir \
                 FROM videos v LEFT JOIN workspaces w ON v.workspace_id = w.id \
                 WHERE v.id IN ({}) ORDER BY INSTR(',{},', ',' || v.id || ',')",
                ids_str, ids_str,
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    if rows.len() != req.video_ids.len() {
        return Err("部分视频不存在".to_string());
    }

    let input_paths: Vec<PathBuf> = rows
        .iter()
        .map(|r| PathBuf::from(r.try_get::<String>("", "file_path").unwrap_or_default()))
        .collect();

    let first_name: String = rows[0].try_get("", "file_name").unwrap_or_default();
    let ws_clip_output_dir: Option<String> = rows[0]
        .try_get::<Option<String>>("", "ws_clip_output_dir")
        .unwrap_or(None);

    // Check compatibility for virtual merge
    if req.mode == "virtual" {
        let ffprobe_path = state.ffmpeg_path.read().unwrap().replace("ffmpeg", "ffprobe");
        let compatible = crate::core::merger::check_merge_compatibility(&ffprobe_path, &input_paths)
            .unwrap_or(false);
        if !compatible {
            return Err(
                "所选视频的编码/分辨率不一致，无法使用快速合并。请使用「重编码合并」模式。".to_string(),
            );
        }
    }

    // Determine output path: user override > workspace clip_output_dir > source dir
    let output_dir = match &req.output_dir {
        Some(dir) => PathBuf::from(dir),
        None => {
            if let Some(ref dir) = ws_clip_output_dir {
                PathBuf::from(dir)
            } else {
                input_paths[0]
                    .parent()
                    .unwrap_or(&PathBuf::from("."))
                    .to_path_buf()
            }
        }
    };

    let stem = first_name.rsplit('.').last().unwrap_or(&first_name);
    let output_filename = format!("{}_merged.mp4", stem);
    let output_path = output_dir.join(&output_filename);

    // Insert task
    let video_ids_json = serde_json::to_string(&req.video_ids).unwrap_or_default();
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO media_tasks (task_type, video_ids, output_path, preset_id, status) \
             VALUES ('merge', '{}', '{}', {}, 'pending')",
            video_ids_json.replace('\'', "''"),
            output_path.to_string_lossy().replace('\'', "''"),
            req.preset_id.map(|id| id.to_string()).unwrap_or("NULL".to_string()),
        ),
    )
    .await
    .map_err(|e| format!("创建合并任务失败: {}", e))?;

    let task_id = get_last_insert_id(state.db.conn()).await?;

    // Submit to task queue
    let ffmpeg_path = state.ffmpeg_path.read().unwrap().clone();
    let db = state.db.clone();
    let output = output_path.clone();
    let mode = req.mode.clone();
    let preset_id = req.preset_id;
    let app = app_handle.clone();

    state
        .task_queue
        .submit(task_id, move |cancel_token, progress_tx| async move {
            let _ = update_media_task_status(&db, task_id, "processing", None).await;

            let progress_tx_clone = progress_tx.clone();
            let on_progress = move |p: crate::core::clipper::ClipProgress| {
                let _ = progress_tx_clone.send(TaskProgressEvent {
                    task_id,
                    status: TaskStatus::Processing,
                    progress: p.progress,
                    message: format!(
                        "合并 {:.0}% (速度: {:.1}x)",
                        p.progress * 100.0,
                        p.speed.unwrap_or(0.0)
                    ),
                });
            };

            let result = if mode == "virtual" {
                crate::core::merger::merge_virtual(
                    &ffmpeg_path,
                    &input_paths,
                    &output,
                    cancel_token,
                    on_progress,
                )
                .await
            } else {
                // Load preset for codec/crf
                let (codec, crf) = if let Some(pid) = preset_id {
                    match load_preset(db.conn(), pid).await {
                        Ok(p) => (p.codec, p.crf),
                        Err(_) => ("auto".to_string(), Some(23u32)),
                    }
                } else {
                    ("auto".to_string(), Some(23u32))
                };

                crate::core::merger::merge_physical(
                    &ffmpeg_path,
                    &input_paths,
                    &output,
                    &codec,
                    crf,
                    cancel_token,
                    on_progress,
                )
                .await
            };

            match &result {
                Ok(()) => {
                    let file_size = std::fs::metadata(&output)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);
                    let _ = update_media_task_status(&db, task_id, "completed", None).await;
                    let _ = app.emit("media-task-completed", serde_json::json!({
                        "task_id": task_id,
                        "task_type": "merge",
                        "output_path": output.to_string_lossy(),
                        "file_size": file_size,
                    }));
                }
                Err(e) => {
                    let status = if e == "Task cancelled" { "cancelled" } else { "failed" };
                    let _ = update_media_task_status(&db, task_id, status, Some(e)).await;
                }
            }

            result
        })
        .await;

    Ok(MediaTaskInfo {
        id: task_id,
        task_type: "merge".to_string(),
        video_ids: req.video_ids,
        output_path: Some(output_path.to_string_lossy().to_string()),
        status: "pending".to_string(),
        progress: 0.0,
        error_message: None,
        created_at: String::new(),
        completed_at: None,
    })
}

// ====== Helpers ======

async fn load_preset(
    conn: &sea_orm::DatabaseConnection,
    preset_id: i64,
) -> Result<PresetOptions, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT options FROM encoding_presets WHERE id = {}", preset_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("预设不存在".to_string())?;

    let options_json: String = row.try_get("", "options").unwrap_or_default();
    serde_json::from_str(&options_json).map_err(|e| format!("预设解析失败: {}", e))
}

async fn get_last_insert_id(conn: &sea_orm::DatabaseConnection) -> Result<i64, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT last_insert_rowid() as id".to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("获取任务ID失败".to_string())?;

    Ok(row.try_get("", "id").unwrap_or(0))
}

async fn update_media_task_status(
    db: &crate::db::Database,
    task_id: i64,
    status: &str,
    error: Option<&String>,
) -> Result<(), String> {
    let error_sql = match error {
        Some(e) => format!(", error_message = '{}'", e.replace('\'', "''")),
        None => String::new(),
    };
    let completed_sql = if status == "completed" {
        ", completed_at = datetime('now')"
    } else {
        ""
    };

    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "UPDATE media_tasks SET status = '{}', progress = {}{}{} WHERE id = {}",
            status,
            if status == "completed" { "1.0" } else { "progress" },
            error_sql,
            completed_sql,
            task_id,
        ),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete output file from disk if it exists
fn delete_media_output_file(path: &str) {
    let p = std::path::Path::new(path);
    if p.exists() {
        if let Err(e) = std::fs::remove_file(p) {
            tracing::warn!("Failed to delete output file {}: {}", path, e);
        } else {
            tracing::info!("Deleted output file: {}", path);
        }
    }
}

/// Delete a single media task (only completed/failed/cancelled)
#[tauri::command]
pub async fn delete_media_task(
    state: State<'_, AppState>,
    task_id: i64,
    delete_files: Option<bool>,
) -> Result<(), String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT status, output_path FROM media_tasks WHERE id = {}", task_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("任务不存在".to_string())?;

    let status: String = row.try_get("", "status").unwrap_or_default();
    if status == "pending" || status == "processing" {
        return Err("请先取消该任务".to_string());
    }

    if delete_files.unwrap_or(false) {
        if let Ok(path) = row.try_get::<String>("", "output_path") {
            delete_media_output_file(&path);
        }
    }

    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM media_tasks WHERE id = {}", task_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Media task deleted: id={}, delete_files={}", task_id, delete_files.unwrap_or(false));
    Ok(())
}

/// Clear all finished media tasks
#[tauri::command]
pub async fn clear_finished_media_tasks(
    state: State<'_, AppState>,
    delete_files: Option<bool>,
) -> Result<u64, String> {
    if delete_files.unwrap_or(false) {
        let rows = sea_orm::ConnectionTrait::query_all(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT output_path FROM media_tasks WHERE status NOT IN ('pending','processing') AND output_path IS NOT NULL".to_string(),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
        for row in &rows {
            if let Ok(path) = row.try_get::<String>("", "output_path") {
                delete_media_output_file(&path);
            }
        }
    }

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        "DELETE FROM media_tasks WHERE status NOT IN ('pending','processing')",
    )
    .await
    .map_err(|e| e.to_string())?;

    let deleted = result.rows_affected();
    tracing::info!("Cleared {} finished media tasks, delete_files={}", deleted, delete_files.unwrap_or(false));
    Ok(deleted)
}
