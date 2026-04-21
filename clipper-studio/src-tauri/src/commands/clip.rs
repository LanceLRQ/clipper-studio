use crate::utils::locks::RwLockExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{Emitter, State};

use crate::core::clipper::{self, BurnOptions, PresetOptions};
use crate::core::queue::{TaskProgressEvent, TaskStatus};
use crate::AppState;

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
    /// Burn danmaku overlay into the output video
    #[serde(default)]
    pub include_danmaku: bool,
    /// Burn subtitle overlay into the output video
    #[serde(default)]
    pub include_subtitle: bool,
    /// Export subtitle as SRT file alongside the output video
    #[serde(default)]
    pub export_subtitle: bool,
    /// Export danmaku as XML file alongside the output video
    #[serde(default)]
    pub export_danmaku: bool,
    /// Batch ID for grouping multiple clips (auto-generated for batch operations)
    #[serde(default)]
    pub batch_id: Option<String>,
    /// Batch display title (auto-generated for batch operations)
    #[serde(default)]
    pub batch_title: Option<String>,
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
    pub output_path: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub batch_id: Option<String>,
    pub batch_title: Option<String>,
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
    // 输入校验（SEC-INPUT-02 / SEC-INPUT-03）
    crate::utils::validation::validate_id(req.video_id, "video_id")?;
    crate::utils::validation::validate_optional_name(req.title.as_deref(), "切片标题")?;

    // Get video info with streamer name
    let video_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.file_path, v.file_name, v.stream_title, v.recorded_at, \
                 st.name as streamer_name, w.clip_output_dir as ws_clip_output_dir \
                 FROM videos v \
                 LEFT JOIN streamers st ON v.streamer_id = st.id \
                 LEFT JOIN workspaces w ON v.workspace_id = w.id \
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
    let ws_clip_output_dir: Option<String> = video_row
        .try_get::<Option<String>>("", "ws_clip_output_dir")
        .unwrap_or(None);

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
        Some(dir) => {
            // 安全校验：用户显式指定的输出目录必须位于已登记的工作区或其 clip_output_dir 下，
            // 防止通过 IPC 将输出写入系统敏感目录（SEC-FS-03）。
            if !state.media_server.is_path_allowed(dir) {
                return Err("输出目录不在工作区允许范围内".to_string());
            }
            PathBuf::from(dir)
        }
        None => {
            // Priority: workspace clip_output_dir > source file directory / clips/
            if let Some(ref dir) = ws_clip_output_dir {
                PathBuf::from(dir)
            } else {
                let src = PathBuf::from(&video_path);
                src.parent().unwrap_or(&PathBuf::from(".")).join("clips")
            }
        }
    };

    // Build output filename: batch clips use simplified name, standalone clips use full name
    let output_filename = if req.batch_id.is_some() {
        let short_name = build_batch_clip_filename(
            req.title.as_deref(),
            recorded_at.as_deref(),
            req.start_ms,
            req.end_ms,
        );
        format!("{}.{}", sanitize_filename(&short_name), ext)
    } else {
        let clip_title = build_clip_name(
            streamer_name.as_deref(),
            stream_title.as_deref(),
            req.title.as_deref(),
            recorded_at.as_deref(),
            req.start_ms,
            req.end_ms,
            &video_name,
        );
        format!("{}.{}", sanitize_filename(&clip_title), ext)
    };
    let output_path = {
        let base = output_dir.join(&output_filename);
        if base.exists() {
            // Append timestamp suffix to avoid overwriting existing files
            let stem = base.file_stem().unwrap_or_default().to_string_lossy();
            let ext = base.extension().unwrap_or_default().to_string_lossy();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let dedup = output_dir.join(format!("{}_{}.{}", stem, ts, ext));
            tracing::info!("Output file already exists, using: {}", dedup.display());
            dedup
        } else {
            base
        }
    };

    // Insert clip_task into database
    let preset_id_sql = req
        .preset_id
        .map(|id| id.to_string())
        .unwrap_or("NULL".to_string());
    let batch_id_sql = req
        .batch_id
        .as_ref()
        .map(|s| format!("'{}'", s.replace('\'', "''")))
        .unwrap_or("NULL".to_string());
    let batch_title_sql = req
        .batch_title
        .as_ref()
        .map(|s| format!("'{}'", s.replace('\'', "''")))
        .unwrap_or("NULL".to_string());
    // Store user-given clip name as task title (for display), not the full filename
    let display_title = req.title.as_deref().unwrap_or("").replace('\'', "''");
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO clip_tasks (video_id, start_time_ms, end_time_ms, title, preset_id, status, batch_id, batch_title, include_danmaku, include_subtitle, export_subtitle, export_danmaku) \
             VALUES ({}, {}, {}, '{}', {}, 'pending', {}, {}, {}, {}, {}, {})",
            req.video_id,
            req.start_ms,
            req.end_ms,
            display_title,
            preset_id_sql,
            batch_id_sql,
            batch_title_sql,
            req.include_danmaku as i32,
            req.include_subtitle as i32,
            req.export_subtitle as i32,
            req.export_danmaku as i32,
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
    let ffmpeg_path = state.ffmpeg_path.read_safe().clone();
    let danmaku_factory_path = state.danmaku_factory_path.read_safe().clone();
    let db = state.db.clone();
    let input_path = PathBuf::from(video_path.clone());
    let output = output_path.clone();
    let start_ms = req.start_ms;
    let end_ms = req.end_ms;
    let video_id = req.video_id;
    let include_danmaku = req.include_danmaku;
    let include_subtitle = req.include_subtitle;
    let export_subtitle = req.export_subtitle;
    let export_danmaku = req.export_danmaku;
    let video_path_clone = video_path.clone();
    let app = app_handle.clone();
    // Use filename stem (without ext) for completion notification
    let clip_title_for_notify = output_filename
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| output_filename.clone());

    state
        .task_queue
        .submit(task_id, move |cancel_token, progress_tx| async move {
            // Update status to processing
            let _ = update_task_status(&db, task_id, "processing", None).await;

            // Prepare burn options
            let burn_options = prepare_burn_options(
                &db,
                &ffmpeg_path,
                &danmaku_factory_path,
                &video_path,
                video_id,
                start_ms,
                end_ms,
                include_danmaku,
                include_subtitle,
                task_id,
            )
            .await;

            let progress_tx_clone = progress_tx.clone();
            let result = clipper::execute_clip_with_burn(
                &ffmpeg_path,
                &input_path,
                &output,
                start_ms,
                end_ms,
                &preset_options,
                &burn_options,
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

            // Cleanup temp ASS files
            if let Some(ref p) = burn_options.danmaku_ass_path {
                let _ = std::fs::remove_file(p);
            }
            if let Some(ref p) = burn_options.subtitle_ass_path {
                let _ = std::fs::remove_file(p);
            }

            let did_burn_danmaku = burn_options.burn_danmaku && burn_options.danmaku_ass_path.is_some();
            let did_burn_subtitle = burn_options.burn_subtitle && burn_options.subtitle_ass_path.is_some();

            match &result {
                Ok(()) => {
                    // Record output file
                    let file_size = std::fs::metadata(&output)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);

                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!(
                            "INSERT INTO clip_outputs (clip_task_id, video_id, output_path, format, variant, file_size, include_danmaku, include_subtitle) \
                             VALUES ({}, {}, '{}', '{}', 'original', {}, {}, {})",
                            task_id,
                            video_id,
                            output.to_string_lossy().replace('\'', "''"),
                            output.extension().unwrap_or_default().to_string_lossy(),
                            file_size,
                            did_burn_danmaku as i32,
                            did_burn_subtitle as i32,
                        ),
                    )
                    .await;

                    // Export subtitle/danmaku files alongside the output video
                    let output_stem = output.with_extension("");
                    if export_subtitle {
                        let srt_path = PathBuf::from(format!("{}.srt", output_stem.display()));
                        match crate::core::subtitle::export_srt_for_clip(
                            &db, video_id, start_ms, end_ms, &srt_path,
                        ).await {
                            Ok(true) => tracing::info!("Exported subtitle SRT: {}", srt_path.display()),
                            Ok(false) => tracing::info!("No subtitles to export for clip [{}-{}ms]", start_ms, end_ms),
                            Err(e) => tracing::warn!("Failed to export subtitle SRT: {}", e),
                        }
                    }
                    if export_danmaku {
                        let xml_path = PathBuf::from(format!("{}.xml", output_stem.display()));
                        let source_xml = std::path::PathBuf::from(&video_path_clone).with_extension("xml");
                        if source_xml.exists() {
                            match crate::core::danmaku::parse_bilibili_xml(&source_xml).await {
                                Ok(result) => {
                                    let scroll_ms = (crate::core::danmaku::DanmakuAssOptions::default().scroll_time * 1000.0) as i64;
                                    let filtered = crate::core::danmaku::filter_danmaku_by_range(&result.items, start_ms, end_ms, scroll_ms);
                                    if !filtered.is_empty() {
                                        match crate::core::danmaku::write_bilibili_xml(&filtered, &xml_path) {
                                            Ok(()) => tracing::info!("Exported danmaku XML: {}", xml_path.display()),
                                            Err(e) => tracing::warn!("Failed to export danmaku XML: {}", e),
                                        }
                                    } else {
                                        tracing::info!("No danmaku to export for clip [{}-{}ms]", start_ms, end_ms);
                                    }
                                }
                                Err(e) => tracing::warn!("Failed to parse danmaku XML for export: {}", e),
                            }
                        }
                    }

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
        title: req.title.clone(),
        status: "pending".to_string(),
        progress: 0.0,
        error_message: None,
        output_path: None,
        created_at: chrono_now(),
        completed_at: None,
        batch_id: req.batch_id.clone(),
        batch_title: req.batch_title.clone(),
    })
}

/// Cancel a clip task
#[tauri::command]
pub async fn cancel_clip(state: State<'_, AppState>, task_id: i64) -> Result<bool, String> {
    crate::utils::validation::validate_id(task_id, "task_id")?;
    let cancelled = state.task_queue.cancel(task_id).await;
    if cancelled {
        let _ = update_task_status(&state.db, task_id, "cancelled", None).await;
    }
    Ok(cancelled)
}

/// Check if there are any active clip tasks (pending or processing)
#[tauri::command]
pub async fn has_active_clip_tasks(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.task_queue.has_active_tasks().await)
}

/// Retry a failed/cancelled/completed clip task: reset status and re-enqueue with original parameters
#[tauri::command]
pub async fn retry_clip_task(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    task_id: i64,
) -> Result<ClipTaskInfo, String> {
    crate::utils::validation::validate_id(task_id, "task_id")?;
    retry_task_internal(&*state, &app_handle, task_id).await
}

/// Internal retry implementation, shared by single-task and batch retry paths.
async fn retry_task_internal(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    task_id: i64,
) -> Result<ClipTaskInfo, String> {
    // Read original task parameters directly from clip_tasks so failed/cancelled
    // tasks can still recover their burn/export settings.
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.video_id, t.start_time_ms, t.end_time_ms, t.title, t.preset_id, t.status, \
                 t.batch_id, t.batch_title, t.include_danmaku, t.include_subtitle, \
                 t.export_subtitle, t.export_danmaku, \
                 (SELECT output_path FROM clip_outputs WHERE clip_task_id = t.id LIMIT 1) AS output_path \
                 FROM clip_tasks t \
                 WHERE t.id = {}",
                task_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("任务不存在".to_string())?;

    let status: String = row.try_get("", "status").unwrap_or_default();
    if status == "pending" || status == "processing" {
        return Err("任务正在执行中，无法重试".to_string());
    }

    let video_id: i64 = row.try_get("", "video_id").unwrap_or(0);
    let start_ms: i64 = row.try_get("", "start_time_ms").unwrap_or(0);
    let end_ms: i64 = row.try_get("", "end_time_ms").unwrap_or(0);
    let title: Option<String> = row
        .try_get("", "title")
        .ok()
        .filter(|s: &String| !s.is_empty());
    let preset_id: Option<i64> = row.try_get("", "preset_id").ok();
    let batch_id: Option<String> = row.try_get("", "batch_id").ok();
    let batch_title: Option<String> = row.try_get("", "batch_title").ok();
    let output_path: Option<String> = row.try_get("", "output_path").ok();
    let include_danmaku: bool = row.try_get::<i32>("", "include_danmaku").unwrap_or(0) != 0;
    let include_subtitle: bool = row.try_get::<i32>("", "include_subtitle").unwrap_or(0) != 0;
    let export_subtitle: bool = row.try_get::<i32>("", "export_subtitle").unwrap_or(0) != 0;
    let export_danmaku: bool = row.try_get::<i32>("", "export_danmaku").unwrap_or(0) != 0;

    // Get video info for re-enqueue (same query as create_clip)
    let video_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.file_path, v.file_name, v.stream_title, v.recorded_at, \
                 st.name as streamer_name, w.clip_output_dir as ws_clip_output_dir \
                 FROM videos v \
                 LEFT JOIN streamers st ON v.streamer_id = st.id \
                 LEFT JOIN workspaces w ON v.workspace_id = w.id \
                 WHERE v.id = {}",
                video_id
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
    let ws_clip_output_dir: Option<String> = video_row
        .try_get::<Option<String>>("", "ws_clip_output_dir")
        .unwrap_or(None);

    // Determine output path: use original if available, otherwise regenerate (same logic as create_clip)
    let output = if let Some(ref op) = output_path {
        PathBuf::from(op)
    } else {
        let output_dir = if let Some(ref dir) = ws_clip_output_dir {
            PathBuf::from(dir)
        } else {
            let src = PathBuf::from(&video_path);
            src.parent().unwrap_or(&PathBuf::from(".")).join("clips")
        };

        let ext = "mp4";
        let output_filename = if batch_id.is_some() {
            let short_name = build_batch_clip_filename(
                title.as_deref(),
                recorded_at.as_deref(),
                start_ms,
                end_ms,
            );
            format!("{}.{}", sanitize_filename(&short_name), ext)
        } else {
            let clip_title = build_clip_name(
                streamer_name.as_deref(),
                stream_title.as_deref(),
                title.as_deref(),
                recorded_at.as_deref(),
                start_ms,
                end_ms,
                &video_name,
            );
            format!("{}.{}", sanitize_filename(&clip_title), ext)
        };
        output_dir.join(&output_filename)
    };

    // Load preset
    let preset_options = if let Some(pid) = preset_id {
        load_preset(state.db.conn(), pid).await?
    } else {
        PresetOptions {
            codec: "copy".to_string(),
            crf: None,
            audio_only: None,
        }
    };

    // Atomically reset task status: only if task is in a terminal state.
    // Completed tasks are allowed so "重新生成" works (will overwrite output).
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "UPDATE clip_tasks SET status = 'pending', progress = 0, error_message = NULL, completed_at = NULL \
             WHERE id = {} AND status IN ('failed', 'cancelled', 'completed')",
            task_id
        ),
    )
    .await
    .map_err(|e| format!("重置任务状态失败: {}", e))?;

    if result.rows_affected() == 0 {
        return Err("任务状态已变更，无法重试（可能正在执行中）".to_string());
    }

    // Delete old clip_outputs records (failed tasks usually have none, but just in case)
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM clip_outputs WHERE clip_task_id = {}", task_id),
    )
    .await
    .ok();

    // Re-enqueue to task queue
    let ffmpeg_path = state.ffmpeg_path.read_safe().clone();
    let danmaku_factory_path = state.danmaku_factory_path.read_safe().clone();
    let db = state.db.clone();
    let input_path = PathBuf::from(video_path.clone());
    let video_path_clone = video_path.clone();
    let output_clone = output.clone();
    let app = app_handle.clone();
    let output_filename = output
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let clip_title_for_notify = output_filename
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| output_filename.clone());

    state
        .task_queue
        .submit(task_id, move |cancel_token, progress_tx| async move {
            let _ = update_task_status(&db, task_id, "processing", None).await;

            let burn_options = prepare_burn_options(
                &db,
                &ffmpeg_path,
                &danmaku_factory_path,
                &video_path,
                video_id,
                start_ms,
                end_ms,
                include_danmaku,
                include_subtitle,
                task_id,
            )
            .await;

            let progress_tx_clone = progress_tx.clone();
            let result = clipper::execute_clip_with_burn(
                &ffmpeg_path,
                &input_path,
                &output_clone,
                start_ms,
                end_ms,
                &preset_options,
                &burn_options,
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

            // Cleanup temp ASS files
            if let Some(ref p) = burn_options.danmaku_ass_path {
                let _ = std::fs::remove_file(p);
            }
            if let Some(ref p) = burn_options.subtitle_ass_path {
                let _ = std::fs::remove_file(p);
            }

            let did_burn_danmaku = burn_options.burn_danmaku && burn_options.danmaku_ass_path.is_some();
            let did_burn_subtitle = burn_options.burn_subtitle && burn_options.subtitle_ass_path.is_some();

            match &result {
                Ok(()) => {
                    let file_size = std::fs::metadata(&output_clone)
                        .map(|m| m.len() as i64)
                        .unwrap_or(0);

                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!(
                            "INSERT INTO clip_outputs (clip_task_id, video_id, output_path, format, variant, file_size, include_danmaku, include_subtitle) \
                             VALUES ({}, {}, '{}', '{}', 'original', {}, {}, {})",
                            task_id,
                            video_id,
                            output_clone.to_string_lossy().replace('\'', "''"),
                            output_clone.extension().unwrap_or_default().to_string_lossy(),
                            file_size,
                            did_burn_danmaku as i32,
                            did_burn_subtitle as i32,
                        ),
                    )
                    .await;

                    // Re-export subtitle/danmaku sidecars if the original task requested them.
                    let output_stem = output_clone.with_extension("");
                    if export_subtitle {
                        let srt_path = PathBuf::from(format!("{}.srt", output_stem.display()));
                        match crate::core::subtitle::export_srt_for_clip(
                            &db, video_id, start_ms, end_ms, &srt_path,
                        ).await {
                            Ok(true) => tracing::info!("Exported subtitle SRT: {}", srt_path.display()),
                            Ok(false) => tracing::info!("No subtitles to export for clip [{}-{}ms]", start_ms, end_ms),
                            Err(e) => tracing::warn!("Failed to export subtitle SRT: {}", e),
                        }
                    }
                    if export_danmaku {
                        let xml_path = PathBuf::from(format!("{}.xml", output_stem.display()));
                        let source_xml = std::path::PathBuf::from(&video_path_clone).with_extension("xml");
                        if source_xml.exists() {
                            match crate::core::danmaku::parse_bilibili_xml(&source_xml).await {
                                Ok(result) => {
                                    let scroll_ms = (crate::core::danmaku::DanmakuAssOptions::default().scroll_time * 1000.0) as i64;
                                    let filtered = crate::core::danmaku::filter_danmaku_by_range(&result.items, start_ms, end_ms, scroll_ms);
                                    if !filtered.is_empty() {
                                        match crate::core::danmaku::write_bilibili_xml(&filtered, &xml_path) {
                                            Ok(()) => tracing::info!("Exported danmaku XML: {}", xml_path.display()),
                                            Err(e) => tracing::warn!("Failed to export danmaku XML: {}", e),
                                        }
                                    } else {
                                        tracing::info!("No danmaku to export for clip [{}-{}ms]", start_ms, end_ms);
                                    }
                                }
                                Err(e) => tracing::warn!("Failed to parse danmaku XML for export: {}", e),
                            }
                        }
                    }

                    let _ = update_task_status(&db, task_id, "completed", None).await;
                    let _ = app.emit("clip-completed", serde_json::json!({
                        "task_id": task_id,
                        "title": clip_title_for_notify,
                        "output_path": output_clone.to_string_lossy(),
                        "file_size": file_size,
                    }));
                }
                Err(e) => {
                    let s = if e == "Task cancelled" { "cancelled" } else { "failed" };
                    let _ = update_task_status(&db, task_id, s, Some(e)).await;
                }
            }

            result
        })
        .await;

    Ok(ClipTaskInfo {
        id: task_id,
        video_id,
        start_time_ms: start_ms,
        end_time_ms: end_ms,
        title,
        status: "pending".to_string(),
        progress: 0.0,
        error_message: None,
        output_path: Some(output.to_string_lossy().to_string()),
        created_at: String::new(),
        completed_at: None,
        batch_id,
        batch_title,
    })
}

/// Retry all finished (failed/cancelled/completed) tasks in a batch.
/// If any task in the batch is still pending/processing, the request is rejected.
#[tauri::command]
pub async fn retry_clip_batch(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    batch_id: String,
) -> Result<usize, String> {
    let escaped = batch_id.replace('\'', "''");

    // Reject if any sibling is still active
    let active_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) AS cnt FROM clip_tasks \
                 WHERE batch_id = '{}' AND status IN ('pending', 'processing')",
                escaped
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("查询批次状态失败".to_string())?;
    let active: i64 = active_row.try_get("", "cnt").unwrap_or(0);
    if active > 0 {
        return Err("批次中有任务正在执行，请等待完成后再重试".to_string());
    }

    // Collect all retryable task ids
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id FROM clip_tasks \
                 WHERE batch_id = '{}' AND status IN ('failed', 'cancelled', 'completed') \
                 ORDER BY id ASC",
                escaped
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Err("批次中没有可重试的任务".to_string());
    }

    let mut success = 0usize;
    let mut last_err: Option<String> = None;
    for row in rows {
        let id: i64 = row.try_get("", "id").unwrap_or(0);
        if id == 0 {
            continue;
        }
        match retry_task_internal(&*state, &app_handle, id).await {
            Ok(_) => success += 1,
            Err(e) => {
                tracing::warn!("Retry task {} in batch {} failed: {}", id, batch_id, e);
                last_err = Some(e);
            }
        }
    }

    if success == 0 {
        return Err(last_err.unwrap_or_else(|| "批次重试失败".to_string()));
    }
    Ok(success)
}

/// List clip tasks with optional video_id filter
#[tauri::command]
pub async fn list_clip_tasks(
    state: State<'_, AppState>,
    video_id: Option<i64>,
    workspace_id: Option<i64>,
    date_from: Option<String>,
    date_to: Option<String>,
) -> Result<Vec<ClipTaskInfo>, String> {
    let mut conditions: Vec<String> = Vec::new();
    if let Some(id) = video_id {
        conditions.push(format!("t.video_id = {}", id));
    }
    if let Some(ws_id) = workspace_id {
        conditions.push(format!("v.workspace_id = {}", ws_id));
    }
    if let Some(ref from) = date_from {
        conditions.push(format!(
            "t.created_at >= '{} 00:00:00'",
            from.replace('\'', "")
        ));
    }
    if let Some(ref to) = date_to {
        conditions.push(format!(
            "t.created_at <= '{} 23:59:59'",
            to.replace('\'', "")
        ));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.*, co.output_path FROM clip_tasks t \
                 LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
                 LEFT JOIN videos v ON t.video_id = v.id \
                 {} ORDER BY t.created_at DESC",
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
            output_path: row
                .try_get::<Option<String>>("", "output_path")
                .unwrap_or(None),
            created_at: row.try_get("", "created_at").unwrap_or_default(),
            completed_at: row.try_get("", "completed_at").ok(),
            batch_id: row.try_get("", "batch_id").ok(),
            batch_title: row.try_get("", "batch_title").ok(),
        })
        .collect();

    Ok(tasks)
}

/// List encoding presets
#[tauri::command]
pub async fn list_presets(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
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
            format!(
                "SELECT options FROM encoding_presets WHERE id = {}",
                preset_id
            ),
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

/// Build clip display name: {主播}-{直播标题}-{片段名}-{开始绝对时间}-{结束绝对时间}
///
/// Time format: yyMMdd-HHmm (e.g. "260101-2130")
/// `clip_name` is the user-facing name like "片段1"
/// `recorded_at` is "yyyy-MM-dd HH:mm:ss", used as base for absolute time calculation
fn build_clip_name(
    streamer: Option<&str>,
    stream_title: Option<&str>,
    clip_name: Option<&str>,
    recorded_at: Option<&str>,
    start_ms: i64,
    end_ms: i64,
    fallback_name: &str,
) -> String {
    let mut parts = Vec::new();

    if let Some(name) = streamer {
        parts.push(name.to_string());
    }

    if let Some(t) = stream_title {
        let truncated = if t.chars().count() > 20 {
            format!("{}...", t.chars().take(20).collect::<String>())
        } else {
            t.to_string()
        };
        parts.push(truncated);
    }

    if let Some(cn) = clip_name {
        parts.push(cn.to_string());
    }

    // Compute absolute start/end time from recorded_at + offset
    // Format: yyMMdd-HHmm (e.g. "260405-2130")
    if let Some(ts) = recorded_at {
        // Parse "yyyy-MM-dd HH:mm:ss" into components
        let ts_parts: Vec<&str> = ts.split(&['-', ' ', ':'][..]).collect();
        if ts_parts.len() >= 6 {
            if let (Ok(y), Ok(mo), Ok(d), Ok(h), Ok(mi), Ok(s)) = (
                ts_parts[0].parse::<i64>(),
                ts_parts[1].parse::<i64>(),
                ts_parts[2].parse::<i64>(),
                ts_parts[3].parse::<i64>(),
                ts_parts[4].parse::<i64>(),
                ts_parts[5].parse::<i64>(),
            ) {
                let base_secs = h * 3600 + mi * 60 + s;
                let fmt_absolute = |offset_ms: i64| -> String {
                    let offset_s = offset_ms / 1000;
                    let total_s = base_secs + offset_s;
                    // Handle day overflow simply
                    let extra_days = total_s / 86400;
                    let day_s = total_s % 86400;
                    let ah = day_s / 3600;
                    let am = (day_s % 3600) / 60;
                    // Simple day offset (not calendar-accurate, good enough for display)
                    let ad = d + extra_days;
                    format!("{:02}{:02}{:02}-{:02}{:02}", y % 100, mo, ad, ah, am)
                };
                parts.push(fmt_absolute(start_ms));
                parts.push(fmt_absolute(end_ms));
            }
        }
    } else {
        // No recorded_at: use file-relative time
        let fmt_relative = |ms: i64| -> String {
            let total_s = ms / 1000;
            let h = total_s / 3600;
            let m = (total_s % 3600) / 60;
            let s = total_s % 60;
            format!("{:02}{:02}{:02}", h, m, s)
        };
        parts.push(format!(
            "{}-{}",
            fmt_relative(start_ms),
            fmt_relative(end_ms)
        ));
    }

    if parts.is_empty() {
        let stem = fallback_name.rsplit('.').last().unwrap_or(fallback_name);
        return format!("{}_{}-{}", stem, start_ms, end_ms);
    }

    parts.join("-")
}

/// Build simplified filename for batch clip: {片段名}-{时间段}
/// With recorded_at: "片段1-260407-2130-260407-2145"
/// Without recorded_at: "片段1-0100-0300"
fn build_batch_clip_filename(
    clip_name: Option<&str>,
    recorded_at: Option<&str>,
    start_ms: i64,
    end_ms: i64,
) -> String {
    let name = clip_name.unwrap_or("clip");

    let time_range = if let Some(ts) = recorded_at {
        // Parse "yyyy-MM-dd HH:mm:ss" and compute absolute time
        let ts_parts: Vec<&str> = ts.split(&['-', ' ', ':'][..]).collect();
        if ts_parts.len() >= 6 {
            if let (Ok(y), Ok(mo), Ok(d), Ok(h), Ok(mi), Ok(s)) = (
                ts_parts[0].parse::<i64>(),
                ts_parts[1].parse::<i64>(),
                ts_parts[2].parse::<i64>(),
                ts_parts[3].parse::<i64>(),
                ts_parts[4].parse::<i64>(),
                ts_parts[5].parse::<i64>(),
            ) {
                let base_secs = h * 3600 + mi * 60 + s;
                let fmt = |offset_ms: i64| -> String {
                    let offset_s = offset_ms / 1000;
                    let total_s = base_secs + offset_s;
                    let extra_days = total_s / 86400;
                    let day_s = total_s % 86400;
                    let ah = day_s / 3600;
                    let am = (day_s % 3600) / 60;
                    let ad = d + extra_days;
                    format!("{:02}{:02}{:02}-{:02}{:02}", y % 100, mo, ad, ah, am)
                };
                format!("{}-{}", fmt(start_ms), fmt(end_ms))
            } else {
                format_relative_time_range(start_ms, end_ms)
            }
        } else {
            format_relative_time_range(start_ms, end_ms)
        }
    } else {
        format_relative_time_range(start_ms, end_ms)
    };

    format!("{}-{}", name, time_range)
}

/// Format relative time range: "HHMMSS-HHMMSS" or "MMSS-MMSS"
fn format_relative_time_range(start_ms: i64, end_ms: i64) -> String {
    let fmt = |ms: i64| -> String {
        let total_s = ms / 1000;
        let h = total_s / 3600;
        let m = (total_s % 3600) / 60;
        let s = total_s % 60;
        if h > 0 {
            format!("{:02}{:02}{:02}", h, m, s)
        } else {
            format!("{:02}{:02}", m, s)
        }
    };
    format!("{}-{}", fmt(start_ms), fmt(end_ms))
}

/// Build batch output subfolder path: {base_dir}/{YYYYMMDD}_{HHmm}_{主播}_{标题}
async fn build_batch_output_dir(
    conn: &sea_orm::DatabaseConnection,
    video_id: i64,
    _ffprobe_path: &str,
) -> Result<PathBuf, String> {
    // Get video info for base output dir and metadata
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.file_path, v.stream_title, st.name as streamer_name, \
                 w.clip_output_dir as ws_clip_output_dir \
                 FROM videos v \
                 LEFT JOIN streamers st ON v.streamer_id = st.id \
                 LEFT JOIN workspaces w ON v.workspace_id = w.id \
                 WHERE v.id = {}",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let video_path: String = row.try_get("", "file_path").unwrap_or_default();
    let streamer_name: Option<String> = row.try_get("", "streamer_name").ok();
    let stream_title: Option<String> = row.try_get("", "stream_title").ok();
    let ws_clip_output_dir: Option<String> = row
        .try_get::<Option<String>>("", "ws_clip_output_dir")
        .unwrap_or(None);

    // Determine base output dir
    let base_dir = if let Some(ref dir) = ws_clip_output_dir {
        PathBuf::from(dir)
    } else {
        let src = PathBuf::from(&video_path);
        src.parent().unwrap_or(&PathBuf::from(".")).join("clips")
    };

    // Get local datetime from SQLite for folder name
    let time_row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT strftime('%Y%m%d', 'now', 'localtime') as date_part, \
                    strftime('%H%M', 'now', 'localtime') as time_part"
                .to_string(),
        ),
    )
    .await
    .ok()
    .flatten();

    let date_part: String = time_row
        .as_ref()
        .and_then(|r| r.try_get("", "date_part").ok())
        .unwrap_or_else(|| "00000000".to_string());
    let time_part: String = time_row
        .as_ref()
        .and_then(|r| r.try_get("", "time_part").ok())
        .unwrap_or_else(|| "0000".to_string());

    // Build folder name: {YYYYMMDD}_{HHmm}_{主播}_{标题}
    let mut folder_parts = vec![format!("{}_{}", date_part, time_part)];

    if let Some(name) = streamer_name {
        if !name.is_empty() {
            folder_parts.push(name);
        }
    }
    if let Some(title) = stream_title {
        if !title.is_empty() {
            let truncated = if title.chars().count() > 20 {
                title.chars().take(20).collect::<String>()
            } else {
                title
            };
            folder_parts.push(truncated);
        }
    }

    let folder_name = sanitize_filename(&folder_parts.join("_"));
    Ok(base_dir.join(folder_name))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// SEC-FS-05：生成带随机后缀的独占临时文件路径。
///
/// 使用 `tempfile::Builder`（内部 O_EXCL）原子创建唯一文件，再 `keep()` 释放
/// auto-delete 守卫把清理责任交给调用方现有的 `remove_file` 逻辑。
/// 避免仅以 `task_id` 作为文件名可被本机其他进程预测导致 TOCTOU 替换攻击。
/// 失败时返回 None，调用方应优雅降级跳过该可选流程。
fn create_random_tmp(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile_in(dir)
        .ok()?
        .keep()
        .ok()
        .map(|(_, path)| path)
}

// ====== Batch Clip Creation ======

#[derive(Debug, Deserialize)]
pub struct BatchClipItem {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: Option<String>,
    pub preset_id: Option<i64>,
    pub offset_before_ms: i64,
    pub offset_after_ms: i64,
    pub audio_only: bool,
    #[serde(default)]
    pub include_danmaku: bool,
    #[serde(default)]
    pub include_subtitle: bool,
    #[serde(default)]
    pub export_subtitle: bool,
    #[serde(default)]
    pub export_danmaku: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateBatchClipsRequest {
    pub video_id: i64,
    pub clips: Vec<BatchClipItem>,
}

/// Create multiple clip tasks at once
#[tauri::command]
pub async fn create_batch_clips(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    req: CreateBatchClipsRequest,
) -> Result<Vec<ClipTaskInfo>, String> {
    // Generate batch ID and title
    let batch_id = format!(
        "batch-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let batch_title = build_batch_title(state.db.conn(), req.video_id).await;

    // Build batch subfolder: determine base output dir, then create subfolder
    let ffprobe_path = state.ffprobe_path.read_safe().clone();
    let batch_output_dir =
        build_batch_output_dir(state.db.conn(), req.video_id, &ffprobe_path).await?;
    // Ensure the subfolder exists
    tokio::fs::create_dir_all(&batch_output_dir)
        .await
        .map_err(|e| format!("无法创建批次输出目录: {}", e))?;

    let mut results = Vec::new();

    for item in &req.clips {
        // Apply offsets
        let effective_start = (item.start_ms - item.offset_before_ms).max(0);
        let effective_end = item.end_ms + item.offset_after_ms;

        // Determine preset: if audio_only, use audio-only preset; else use specified or default
        let preset_id = if item.audio_only {
            // Find the audio-only builtin preset
            sea_orm::ConnectionTrait::query_one(
                state.db.conn(),
                sea_orm::Statement::from_string(
                    sea_orm::DatabaseBackend::Sqlite,
                    "SELECT id FROM encoding_presets WHERE name = '仅音频' AND is_builtin = 1"
                        .to_string(),
                ),
            )
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<i64>("", "id").ok())
        } else {
            item.preset_id
        };

        let clip_req = CreateClipRequest {
            video_id: req.video_id,
            start_ms: effective_start,
            end_ms: effective_end,
            title: item.title.clone(),
            preset_id,
            output_dir: Some(batch_output_dir.to_string_lossy().to_string()),
            include_danmaku: item.include_danmaku,
            include_subtitle: item.include_subtitle,
            export_subtitle: item.export_subtitle,
            export_danmaku: item.export_danmaku,
            batch_id: Some(batch_id.clone()),
            batch_title: Some(batch_title.clone()),
        };

        match create_clip(state.clone(), app_handle.clone(), clip_req).await {
            Ok(info) => results.push(info),
            Err(e) => {
                tracing::warn!("Batch clip item failed: {}", e);
                // Continue with remaining clips
            }
        }
    }

    Ok(results)
}

/// Prepare burn options for a clip task.
///
/// Generates temporary ASS files for danmaku and/or subtitles if requested.
async fn prepare_burn_options(
    db: &crate::db::Database,
    _ffmpeg_path: &str,
    danmaku_factory_path: &str,
    video_path: &str,
    video_id: i64,
    start_ms: i64,
    end_ms: i64,
    include_danmaku: bool,
    include_subtitle: bool,
    task_id: i64,
) -> BurnOptions {
    let tmp_dir = std::env::temp_dir();
    let mut danmaku_ass_path: Option<PathBuf> = None;
    let mut subtitle_ass_path: Option<PathBuf> = None;

    // Generate danmaku ASS
    if include_danmaku && !danmaku_factory_path.is_empty() {
        let xml_path = std::path::PathBuf::from(video_path).with_extension("xml");
        if xml_path.exists() {
            match crate::core::danmaku::parse_bilibili_xml(&xml_path).await {
                Ok(result) => {
                    // Filter to clip range and shift timestamps
                    let scroll_ms = (crate::core::danmaku::DanmakuAssOptions::default().scroll_time
                        * 1000.0) as i64;
                    let filtered = crate::core::danmaku::filter_danmaku_by_range(
                        &result.items,
                        start_ms,
                        end_ms,
                        scroll_ms,
                    );
                    if !filtered.is_empty() {
                        // SEC-FS-05：随机名独占临时文件替代 task_id 预测性命名
                        let prefix = format!("clipper_{}_", task_id);
                        let tmp_files = create_random_tmp(&tmp_dir, &prefix, "_danmaku.xml")
                            .zip(create_random_tmp(&tmp_dir, &prefix, "_danmaku.ass"));
                        if let Some((tmp_xml, tmp_ass)) = tmp_files {
                            if crate::core::danmaku::write_bilibili_xml(&filtered, &tmp_xml)
                                .is_ok()
                            {
                                let options = crate::core::danmaku::DanmakuAssOptions::default();
                                match crate::core::danmaku::convert_to_ass(
                                    danmaku_factory_path,
                                    &tmp_xml,
                                    &tmp_ass,
                                    &options,
                                )
                                .await
                                {
                                    Ok(()) => {
                                        danmaku_ass_path = Some(tmp_ass);
                                        tracing::info!(
                                            "Generated danmaku ASS for task {}",
                                            task_id
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "DanmakuFactory conversion failed: {}",
                                            e
                                        );
                                        // 转换失败时清理预创建的空 ASS 占位
                                        let _ = std::fs::remove_file(&tmp_ass);
                                    }
                                }
                            } else {
                                // XML 写入失败：清理预创建的空 ASS 占位
                                let _ = std::fs::remove_file(&tmp_ass);
                            }
                            let _ = std::fs::remove_file(&tmp_xml);
                        } else {
                            tracing::warn!(
                                "Skipping danmaku burn: temp file creation failed for task {}",
                                task_id
                            );
                        }
                    } else {
                        tracing::info!("No danmaku in clip range [{}-{}ms]", start_ms, end_ms);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse danmaku XML: {}", e);
                }
            }
        } else {
            tracing::info!("No danmaku XML found for video: {}", video_path);
        }
    }

    // Generate subtitle ASS
    if include_subtitle {
        // SEC-FS-05：随机名独占临时文件
        if let Some(tmp_ass) = create_random_tmp(
            &tmp_dir,
            &format!("clipper_{}_", task_id),
            "_subtitle.ass",
        ) {
            match crate::core::subtitle::export_ass_for_clip(
                db, video_id, start_ms, end_ms, &tmp_ass,
            )
            .await
            {
                Ok(true) => {
                    subtitle_ass_path = Some(tmp_ass);
                    tracing::info!("Generated subtitle ASS for task {}", task_id);
                }
                Ok(false) => {
                    // 无字幕，清理空占位文件
                    let _ = std::fs::remove_file(&tmp_ass);
                    tracing::info!("No subtitles in clip range [{}-{}ms]", start_ms, end_ms);
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_ass);
                    tracing::warn!("Failed to generate subtitle ASS: {}", e);
                }
            }
        } else {
            tracing::warn!(
                "Skipping subtitle burn: temp file creation failed for task {}",
                task_id
            );
        }
    }

    // Determine burn codec — use "auto" (will pick best available hardware encoder)
    let burn_codec = "auto".to_string();
    let burn_crf = Some(23u32);

    BurnOptions {
        burn_danmaku: include_danmaku,
        burn_subtitle: include_subtitle,
        danmaku_ass_path,
        subtitle_ass_path,
        burn_codec,
        burn_crf,
    }
}

/// Check what burn options are available for a video
#[derive(Debug, Serialize)]
pub struct BurnAvailability {
    /// Whether a danmaku XML file exists alongside the video
    pub has_danmaku_xml: bool,
    /// Whether the video has ASR subtitles in the database
    pub has_subtitle: bool,
    /// Whether DanmakuFactory is installed and available
    pub has_danmaku_factory: bool,
}

#[tauri::command]
pub async fn check_video_burn_availability(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<BurnAvailability, String> {
    // Get video file path
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT file_path, has_subtitle FROM videos WHERE id = {}",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let file_path: String = row.try_get("", "file_path").unwrap_or_default();
    let has_subtitle: bool = row.try_get("", "has_subtitle").unwrap_or(false);

    // Check danmaku XML existence
    let xml_path = std::path::PathBuf::from(&file_path).with_extension("xml");
    let has_danmaku_xml = xml_path.exists();

    // Check DanmakuFactory availability
    let has_danmaku_factory = !state.danmaku_factory_path.read_safe().is_empty();

    Ok(BurnAvailability {
        has_danmaku_xml,
        has_subtitle,
        has_danmaku_factory,
    })
}

/// Auto-detect segments by finding silence gaps in the audio envelope
#[tauri::command]
pub async fn auto_segment(
    state: State<'_, AppState>,
    video_id: i64,
    silence_threshold: Option<f32>,
    min_silence_ms: Option<i64>,
    min_segment_ms: Option<i64>,
) -> Result<Vec<crate::core::segment::DetectedSegment>, String> {
    use crate::core::segment::{self, SegmentParams};
    use crate::utils::ffmpeg::AudioEnvelope;

    // Read audio envelope from database
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT window_ms, data FROM audio_envelopes WHERE video_id = {}",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("该视频没有音量数据，请先提取音量包络".to_string())?;

    let window_ms: i32 = row.try_get("", "window_ms").unwrap_or(500);
    let data_bytes: Vec<u8> = row.try_get("", "data").unwrap_or_default();

    // Decode envelope data (stored as f32le blob)
    let values: Vec<f32> = data_bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    if values.is_empty() {
        return Err("音量数据为空".to_string());
    }

    let envelope = AudioEnvelope {
        window_ms: window_ms as u32,
        values,
    };

    let params = SegmentParams {
        silence_threshold: silence_threshold.unwrap_or(0.05),
        min_silence_ms: min_silence_ms.unwrap_or(3000),
        min_segment_ms: min_segment_ms.unwrap_or(10000),
    };

    let segments = segment::detect_segments(&envelope, &params);
    tracing::info!(
        "Auto-segment: detected {} segments for video {} (threshold={}, min_silence={}ms, min_segment={}ms)",
        segments.len(),
        video_id,
        params.silence_threshold,
        params.min_silence_ms,
        params.min_segment_ms,
    );

    Ok(segments)
}

/// Build batch title: [主播] 标题 - yyyy年M月d日 HH:mm
async fn build_batch_title(conn: &sea_orm::DatabaseConnection, video_id: i64) -> String {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.stream_title, st.name as streamer_name \
                 FROM videos v LEFT JOIN streamers st ON v.streamer_id = st.id \
                 WHERE v.id = {}",
                video_id,
            ),
        ),
    )
    .await
    .ok()
    .flatten();

    let streamer_name: Option<String> = row
        .as_ref()
        .and_then(|r| r.try_get("", "streamer_name").ok());
    let stream_title: Option<String> = row
        .as_ref()
        .and_then(|r| r.try_get("", "stream_title").ok());

    // Get local time from SQLite
    let now_str = query_local_datetime_cn(conn).await;

    let mut result = String::new();

    if let Some(name) = streamer_name {
        if !name.is_empty() {
            result.push_str(&format!("[{}] ", name));
        }
    }

    if let Some(title) = stream_title {
        if !title.is_empty() {
            let truncated = if title.chars().count() > 20 {
                format!("{}...", title.chars().take(20).collect::<String>())
            } else {
                title
            };
            result.push_str(&truncated);
        }
    }

    if result.is_empty() {
        result.push_str("切片任务");
    }

    result.push_str(&format!(" - {}", now_str));
    result
}

/// Query local datetime from SQLite and format as "yyyy年M月d日 HH:mm"
async fn query_local_datetime_cn(conn: &sea_orm::DatabaseConnection) -> String {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT strftime('%Y', 'now', 'localtime') as y, \
                    strftime('%m', 'now', 'localtime') as mo, \
                    strftime('%d', 'now', 'localtime') as d, \
                    strftime('%H', 'now', 'localtime') as h, \
                    strftime('%M', 'now', 'localtime') as mi"
                .to_string(),
        ),
    )
    .await
    .ok()
    .flatten();

    match row {
        Some(r) => {
            let y: String = r.try_get("", "y").unwrap_or_default();
            let mo: String = r.try_get("", "mo").unwrap_or_default();
            let d: String = r.try_get("", "d").unwrap_or_default();
            let h: String = r.try_get("", "h").unwrap_or_default();
            let mi: String = r.try_get("", "mi").unwrap_or_default();
            // Remove leading zeros for month and day
            let mo_num: u32 = mo.parse().unwrap_or(0);
            let d_num: u32 = d.parse().unwrap_or(0);
            format!("{}年{}月{}日 {}:{}", y, mo_num, d_num, h, mi)
        }
        None => "未知时间".to_string(),
    }
}

#[derive(Debug, Serialize)]
pub struct DeleteBatchResult {
    pub deleted: u64,
    pub skipped: u64,
}

/// Collect output file paths for given clip task IDs and delete them from disk
fn delete_output_files(db_rows: &[sea_orm::QueryResult]) {
    for row in db_rows {
        let path: String = match row.try_get("", "output_path") {
            Ok(p) => p,
            Err(_) => continue,
        };
        let p = std::path::Path::new(&path);
        if p.exists() {
            if let Err(e) = std::fs::remove_file(p) {
                tracing::warn!("Failed to delete output file {}: {}", path, e);
            } else {
                tracing::info!("Deleted output file: {}", path);
            }
        }
    }
}

/// Delete a single clip task (only completed/failed/cancelled)
#[tauri::command]
pub async fn delete_clip_task(
    state: State<'_, AppState>,
    task_id: i64,
    delete_files: Option<bool>,
) -> Result<(), String> {
    crate::utils::validation::validate_id(task_id, "task_id")?;
    // Check task status
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT status FROM clip_tasks WHERE id = {}", task_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("任务不存在".to_string())?;

    let status: String = row.try_get("", "status").unwrap_or_default();
    if status == "pending" || status == "processing" {
        return Err("请先取消该任务".to_string());
    }

    // Optionally delete output files from disk
    if delete_files.unwrap_or(false) {
        let rows = sea_orm::ConnectionTrait::query_all(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT output_path FROM clip_outputs WHERE clip_task_id = {}",
                    task_id
                ),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
        delete_output_files(&rows);
    }

    // Delete clip_outputs first (FK dependency)
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM clip_outputs WHERE clip_task_id = {}", task_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM clip_tasks WHERE id = {}", task_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!(
        "Clip task deleted: id={}, delete_files={}",
        task_id,
        delete_files.unwrap_or(false)
    );
    Ok(())
}

/// Delete all deletable tasks in a batch
#[tauri::command]
pub async fn delete_clip_batch(
    state: State<'_, AppState>,
    batch_id: String,
    delete_files: Option<bool>,
) -> Result<DeleteBatchResult, String> {
    let escaped = batch_id.replace('\'', "''");

    // Count active tasks in the batch
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT SUM(CASE WHEN status IN ('pending','processing') THEN 1 ELSE 0 END) as active \
                 FROM clip_tasks WHERE batch_id = '{}'",
                escaped
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("批次不存在".to_string())?;

    let active: i64 = row.try_get("", "active").unwrap_or(0);

    // Optionally delete output files from disk
    if delete_files.unwrap_or(false) {
        let rows = sea_orm::ConnectionTrait::query_all(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT output_path FROM clip_outputs WHERE clip_task_id IN (\
                     SELECT id FROM clip_tasks WHERE batch_id = '{}' AND status NOT IN ('pending','processing'))",
                    escaped
                ),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
        delete_output_files(&rows);
    }

    // Delete clip_outputs for deletable tasks
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "DELETE FROM clip_outputs WHERE clip_task_id IN (\
             SELECT id FROM clip_tasks WHERE batch_id = '{}' AND status NOT IN ('pending','processing'))",
            escaped
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Delete the tasks themselves
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "DELETE FROM clip_tasks WHERE batch_id = '{}' AND status NOT IN ('pending','processing')",
            escaped
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let deleted = result.rows_affected();
    tracing::info!(
        "Batch '{}' deleted: {} tasks, {} skipped, delete_files={}",
        batch_id,
        deleted,
        active,
        delete_files.unwrap_or(false)
    );

    Ok(DeleteBatchResult {
        deleted,
        skipped: active as u64,
    })
}

/// Clear all finished (completed/failed/cancelled) clip tasks
#[tauri::command]
pub async fn clear_finished_clip_tasks(
    state: State<'_, AppState>,
    delete_files: Option<bool>,
) -> Result<u64, String> {
    // Optionally delete output files from disk
    if delete_files.unwrap_or(false) {
        let rows = sea_orm::ConnectionTrait::query_all(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT output_path FROM clip_outputs WHERE clip_task_id IN (\
                 SELECT id FROM clip_tasks WHERE status NOT IN ('pending','processing'))"
                    .to_string(),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
        delete_output_files(&rows);
    }

    // Delete clip_outputs for finished tasks
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        "DELETE FROM clip_outputs WHERE clip_task_id IN (\
         SELECT id FROM clip_tasks WHERE status NOT IN ('pending','processing'))",
    )
    .await
    .map_err(|e| e.to_string())?;

    // Delete finished tasks
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        "DELETE FROM clip_tasks WHERE status NOT IN ('pending','processing')",
    )
    .await
    .map_err(|e| e.to_string())?;

    let deleted = result.rows_affected();
    tracing::info!(
        "Cleared {} finished clip tasks, delete_files={}",
        deleted,
        delete_files.unwrap_or(false)
    );
    Ok(deleted)
}

fn chrono_now() -> String {
    // Simple ISO format without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", now) // Simplified; DB uses datetime('now') anyway
}
