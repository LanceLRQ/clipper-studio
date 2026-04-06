use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{Emitter, State};

use crate::AppState;
use crate::core::clipper::{self, PresetOptions};
use crate::core::queue::{TaskProgressEvent, TaskStatus};

#[derive(Debug, Deserialize)]
pub struct CreateClipRequest {
    pub video_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: Option<String>,
    /// Preset ID from encoding_presets table, or None for default
    pub preset_id: Option<i64>,
    /// Output directory override (None = default workspace/clips/)
    pub output_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClipTaskInfo {
    pub id: i64,
    pub video_id: i64,
    pub start_time_ms: i64,
    pub end_time_ms: i64,
    pub title: Option<String>,
    pub status: String,
    pub progress: f64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClipOutputInfo {
    pub id: i64,
    pub clip_task_id: i64,
    pub output_path: String,
    pub format: String,
    pub variant: String,
    pub file_size: Option<i64>,
}

/// Create a clip task: enqueues FFmpeg clip operation
#[tauri::command]
pub async fn create_clip(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    req: CreateClipRequest,
) -> Result<ClipTaskInfo, String> {
    // Get video info with streamer name
    let video_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.file_path, v.file_name, v.stream_title, v.recorded_at, \
                 st.name as streamer_name \
                 FROM videos v LEFT JOIN streamers st ON v.streamer_id = st.id \
                 WHERE v.id = {}",
                req.video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let video_path: String = video_row.try_get("", "file_path").unwrap_or_default();
    let video_name: String = video_row.try_get("", "file_name").unwrap_or_default();
    let streamer_name: Option<String> = video_row.try_get("", "streamer_name").ok();
    let stream_title: Option<String> = video_row.try_get("", "stream_title").ok();
    let recorded_at: Option<String> = video_row.try_get("", "recorded_at").ok();

    // Load preset options
    let preset_options = if let Some(pid) = req.preset_id {
        load_preset(state.db.conn(), pid).await?
    } else {
        // Default: stream copy (fastest)
        PresetOptions {
            codec: "copy".to_string(),
            crf: None,
            audio_only: None,
        }
    };

    // Determine output path
    let ext = if preset_options.audio_only.unwrap_or(false) {
        "m4a"
    } else {
        "mp4"
    };

    let output_dir = match &req.output_dir {
        Some(dir) => PathBuf::from(dir),
        None => {
            // Default: same directory as source, under clips/ subfolder
            let src = PathBuf::from(&video_path);
            src.parent().unwrap_or(&PathBuf::from(".")).join("clips")
        }
    };

    let clip_title = req.title.clone().unwrap_or_else(|| {
        build_clip_name(
            streamer_name.as_deref(),
            stream_title.as_deref(),
            recorded_at.as_deref(),
            req.start_ms,
            req.end_ms,
            &video_name,
        )
    });

    let output_filename = format!("{}.{}", sanitize_filename(&clip_title), ext);
    let output_path = output_dir.join(&output_filename);

    // Insert clip_task into database
    let preset_id_sql = req.preset_id.map(|id| id.to_string()).unwrap_or("NULL".to_string());
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO clip_tasks (video_id, start_time_ms, end_time_ms, title, preset_id, status) \
             VALUES ({}, {}, {}, '{}', {}, 'pending')",
            req.video_id,
            req.start_ms,
            req.end_ms,
            clip_title.replace('\'', "''"),
            preset_id_sql,
        ),
    )
    .await
    .map_err(|e| format!("创建切片任务失败: {}", e))?;

    // Get the task ID
    let task_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT last_insert_rowid() as id".to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("获取任务ID失败".to_string())?;

    let task_id: i64 = task_row.try_get("", "id").unwrap_or(0);

    // Submit to task queue
    let ffmpeg_path = state.ffmpeg_path.clone();
    let db = state.db.clone();
    let input_path = PathBuf::from(video_path);
    let output = output_path.clone();
    let start_ms = req.start_ms;
    let end_ms = req.end_ms;
    let video_id = req.video_id;
    let app = app_handle.clone();
    let clip_title_for_notify = clip_title.clone();

    state
        .task_queue
        .submit(task_id, move |cancel_token, progress_tx| async move {
            // Update status to processing
            let _ = update_task_status(&db, task_id, "processing", None).await;

            let progress_tx_clone = progress_tx.clone();
            let result = clipper::execute_clip(
                &ffmpeg_path,
                &input_path,
                &output,
                start_ms,
                end_ms,
                &preset_options,
                cancel_token,
                move |p| {
                    let _ = progress_tx_clone.send(TaskProgressEvent {
                        task_id,
                        status: TaskStatus::Processing,
                        progress: p.progress,
                        message: format!(
                            "{:.0}% (速度: {:.1}x)",
                            p.progress * 100.0,
                            p.speed.unwrap_or(0.0)
                        ),
                    });
                },
            )
            .await;

            match &result {
                Ok(()) => {
                    // Record output file
                    let file_size = std::fs::metadata(&output)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);

                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!(
                            "INSERT INTO clip_outputs (clip_task_id, video_id, output_path, format, variant, file_size) \
                             VALUES ({}, {}, '{}', '{}', 'original', {})",
                            task_id,
                            video_id,
                            output.to_string_lossy().replace('\'', "''"),
                            output.extension().unwrap_or_default().to_string_lossy(),
                            file_size,
                        ),
                    )
                    .await;

                    let _ = update_task_status(&db, task_id, "completed", None).await;

                    // Notify frontend
                    let _ = app.emit("clip-completed", serde_json::json!({
                        "task_id": task_id,
                        "title": clip_title_for_notify,
                        "output_path": output.to_string_lossy(),
                        "file_size": file_size,
                    }));
                }
                Err(e) => {
                    let status = if e == "Task cancelled" { "cancelled" } else { "failed" };
                    let _ = update_task_status(&db, task_id, status, Some(e)).await;
                }
            }

            result
        })
        .await;

    // Return task info
    Ok(ClipTaskInfo {
        id: task_id,
        video_id: req.video_id,
        start_time_ms: req.start_ms,
        end_time_ms: req.end_ms,
        title: Some(clip_title),
        status: "pending".to_string(),
        progress: 0.0,
        error_message: None,
        created_at: chrono_now(),
        completed_at: None,
    })
}

/// Cancel a clip task
#[tauri::command]
pub async fn cancel_clip(
    state: State<'_, AppState>,
    task_id: i64,
) -> Result<bool, String> {
    let cancelled = state.task_queue.cancel(task_id).await;
    if cancelled {
        let _ = update_task_status(&state.db, task_id, "cancelled", None).await;
    }
    Ok(cancelled)
}

/// List clip tasks with optional video_id filter
#[tauri::command]
pub async fn list_clip_tasks(
    state: State<'_, AppState>,
    video_id: Option<i64>,
) -> Result<Vec<ClipTaskInfo>, String> {
    let where_clause = match video_id {
        Some(id) => format!("WHERE video_id = {}", id),
        None => String::new(),
    };

    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM clip_tasks {} ORDER BY created_at DESC",
                where_clause
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let tasks = rows
        .iter()
        .map(|row| ClipTaskInfo {
            id: row.try_get("", "id").unwrap_or(0),
            video_id: row.try_get("", "video_id").unwrap_or(0),
            start_time_ms: row.try_get("", "start_time_ms").unwrap_or(0),
            end_time_ms: row.try_get("", "end_time_ms").unwrap_or(0),
            title: row.try_get("", "title").ok(),
            status: row.try_get("", "status").unwrap_or_default(),
            progress: row.try_get("", "progress").unwrap_or(0.0),
            error_message: row.try_get("", "error_message").ok(),
            created_at: row.try_get("", "created_at").unwrap_or_default(),
            completed_at: row.try_get("", "completed_at").ok(),
        })
        .collect();

    Ok(tasks)
}

/// List encoding presets
#[tauri::command]
pub async fn list_presets(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT * FROM encoding_presets ORDER BY sort_order".to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let presets = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.try_get::<i64>("", "id").unwrap_or(0),
                "name": row.try_get::<String>("", "name").unwrap_or_default(),
                "category": row.try_get::<String>("", "category").unwrap_or_default(),
                "options": row.try_get::<String>("", "options").unwrap_or_default(),
                "is_builtin": row.try_get::<bool>("", "is_builtin").unwrap_or(false),
            })
        })
        .collect();

    Ok(presets)
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

async fn update_task_status(
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
            "UPDATE clip_tasks SET status = '{}'{}{} WHERE id = {}",
            status, error_sql, completed_sql, task_id,
        ),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Build clip filename: {主播}_{日期}_{标题}_{时间段}
fn build_clip_name(
    streamer: Option<&str>,
    title: Option<&str>,
    recorded_at: Option<&str>,
    start_ms: i64,
    end_ms: i64,
    fallback_name: &str,
) -> String {
    let mut parts = Vec::new();

    if let Some(name) = streamer {
        parts.push(name.to_string());
    }

    // Extract date from recorded_at "yyyy-MM-dd HH:mm:ss" → "yyyyMMdd"
    if let Some(ts) = recorded_at {
        let date_part: String = ts.chars().take(10).filter(|c| *c != '-').collect();
        if date_part.len() == 8 {
            parts.push(date_part);
        }
    }

    if let Some(t) = title {
        // Truncate long titles
        let truncated = if t.chars().count() > 20 {
            format!("{}...", t.chars().take(20).collect::<String>())
        } else {
            t.to_string()
        };
        parts.push(truncated);
    }

    // Time range: HH:mm:ss-HH:mm:ss
    let fmt_time = |ms: i64| -> String {
        let total_s = ms / 1000;
        let h = total_s / 3600;
        let m = (total_s % 3600) / 60;
        let s = total_s % 60;
        format!("{:02}{:02}{:02}", h, m, s)
    };
    parts.push(format!("{}-{}", fmt_time(start_ms), fmt_time(end_ms)));

    if parts.is_empty() {
        let stem = fallback_name.rsplit('.').last().unwrap_or(fallback_name);
        return format!("{}_{}-{}", stem, start_ms, end_ms);
    }

    parts.join("_")
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn chrono_now() -> String {
    // Simple ISO format without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", now) // Simplified; DB uses datetime('now') anyway
}
