use crate::utils::locks::RwLockExt;
use std::path::Path;
use std::sync::Arc;

use tauri::{Emitter, State};

use crate::asr::docker::{self, DockerCapability};
use crate::asr::local::LocalASRProvider;
use crate::asr::manager::{
    ASRLaunchMode, ASRPathValidation, ASRServiceManager, ASRServiceStatusInfo, ASRStartConfig,
};
use crate::asr::provider::{ASRHealthInfo, ASRProvider};
use crate::asr::queue::ASRQueueItem;
use crate::asr::remote::RemoteASRProvider;
use crate::asr::service::{self, ASRTaskInfo, SubtitleSearchResult};
use crate::core::subtitle::SubtitleSegment;
use crate::AppState;

/// Validate ASR remote URL scheme: only `http://` and `https://` are allowed.
/// Rejects `file://`, `javascript:`, and other schemes that could be abused.
fn validate_asr_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("远程 ASR 地址不能为空".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(format!(
            "远程 ASR 地址协议不受支持，仅允许 http:// 或 https://（当前：{}）",
            trimmed
        ));
    }
    Ok(())
}

/// Helper: read a setting from settings_kv.
/// 命中 `is_secret_key` 的 key 会透明 base64 解码（兼容旧明文）。
async fn read_setting(state: &AppState, key: &str) -> Option<String> {
    let raw = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT value FROM settings_kv WHERE key = '{}'",
                key
            ),
        ),
    )
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get::<String>("", "value").ok())?;

    if crate::utils::secrets::is_secret_key(key) {
        Some(crate::utils::secrets::deobfuscate(&raw))
    } else {
        Some(raw)
    }
}

/// Get or create ASR provider based on settings_kv configuration.
///
/// Settings keys:
/// - `asr_mode`: "local" (default) | "remote" | "disabled"
/// - `asr_port`: local ASR port (default 8765)
/// - `asr_url`: remote ASR base URL
/// - `asr_api_key`: remote ASR API key
async fn get_provider(state: &AppState) -> Result<Arc<dyn ASRProvider>, String> {
    let mode = read_setting(state, "asr_mode")
        .await
        .unwrap_or("local".to_string());

    tracing::info!("[ASR] get_provider: mode={}", mode);

    match mode.as_str() {
        "disabled" => Err("ASR 功能已禁用，请在设置中启用".to_string()),
        "remote" => {
            let url = read_setting(state, "asr_url")
                .await
                .ok_or("请先在设置中配置远程 ASR 地址")?;
            validate_asr_url(&url)?;
            tracing::info!("[ASR] Using remote provider: {}", url);
            let api_key = read_setting(state, "asr_api_key").await;
            Ok(Arc::new(RemoteASRProvider::new(&url, api_key)))
        }
        _ => {
            // "local" or default
            let port: u16 = read_setting(state, "asr_port")
                .await
                .and_then(|v| v.parse().ok())
                .unwrap_or(8765);
            tracing::info!("[ASR] Using local provider: http://127.0.0.1:{}", port);
            Ok(Arc::new(LocalASRProvider::new(port)))
        }
    }
}

/// Submit an ASR task for a video
#[tauri::command]
pub async fn submit_asr(
    state: State<'_, AppState>,
    video_id: i64,
    language: Option<String>,
    force: Option<bool>,
) -> Result<i64, String> {
    let provider = get_provider(&state).await?;
    // Use frontend-provided language, or fall back to settings, or default "Chinese"
    let lang = match language {
        Some(l) if !l.is_empty() => l,
        _ => read_setting(&state, "asr_language")
            .await
            .unwrap_or_else(|| "Chinese".to_string()),
    };
    let ffmpeg_path = state.ffmpeg_path.read_safe().clone();
    service::submit_asr(
        &state.db,
        &provider,
        &ffmpeg_path,
        video_id,
        Some(lang.as_str()),
        force.unwrap_or(false),
    )
    .await
}

/// Poll ASR task status (call periodically from frontend)
#[tauri::command]
pub async fn poll_asr(
    state: State<'_, AppState>,
    asr_task_id: i64,
) -> Result<ASRTaskInfo, String> {
    let provider = get_provider(&state).await?;
    service::poll_asr(&state.db, &provider, asr_task_id).await
}

/// List ASR tasks for a video
#[tauri::command]
pub async fn list_asr_tasks(
    state: State<'_, AppState>,
    video_id: Option<i64>,
) -> Result<Vec<ASRTaskInfo>, String> {
    service::list_asr_tasks(&state.db, video_id).await
}

/// List subtitle segments for a video, along with base_ms for time conversion
#[tauri::command]
pub async fn list_subtitles(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<SubtitleListResponse, String> {
    let segments = service::list_subtitles(&state.db, video_id).await?;
    let base_ms = crate::core::subtitle::get_base_ms(&state.db, video_id).await;
    Ok(SubtitleListResponse { segments, base_ms })
}

#[derive(serde::Serialize)]
pub struct SubtitleListResponse {
    pub segments: Vec<SubtitleSegment>,
    pub base_ms: i64,
}

/// Search subtitles by text (FTS5 full-text search)
#[tauri::command]
pub async fn search_subtitles(
    state: State<'_, AppState>,
    query: String,
    video_id: Option<i64>,
) -> Result<Vec<SubtitleSegment>, String> {
    service::search_subtitles(&state.db, &query, video_id).await
}

/// Search subtitles globally with video metadata (FTS5)
#[tauri::command]
pub async fn search_subtitles_global(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SubtitleSearchResult>, String> {
    service::search_subtitles_global(&state.db, &query).await
}

/// Check ASR engine health
#[tauri::command]
pub async fn check_asr_health(
    state: State<'_, AppState>,
) -> Result<ASRHealthInfo, String> {
    tracing::info!("[ASR] check_asr_health called");
    let provider = get_provider(&state).await?;
    let result = provider.health().await;
    match &result {
        Ok(info) => tracing::info!("[ASR] check_asr_health OK: {:?}", info),
        Err(e) => tracing::warn!("[ASR] check_asr_health FAILED: {}", e),
    }
    result
}

// ==================== Subtitle Editing ====================

/// Update a subtitle segment's text and time range
#[tauri::command]
pub async fn update_subtitle(
    state: State<'_, AppState>,
    segment_id: i64,
    text: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<(), String> {
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "UPDATE subtitle_segments SET text = '{}', start_ms = {}, end_ms = {} WHERE id = {}",
            text.replace('\'', "''"),
            start_ms,
            end_ms,
            segment_id,
        ),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a subtitle segment
#[tauri::command]
pub async fn delete_subtitle(
    state: State<'_, AppState>,
    segment_id: i64,
) -> Result<(), String> {
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM subtitle_segments WHERE id = {}", segment_id),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Merge multiple consecutive subtitle segments into one
#[tauri::command]
pub async fn merge_subtitles(
    state: State<'_, AppState>,
    segment_ids: Vec<i64>,
) -> Result<SubtitleSegment, String> {
    if segment_ids.len() < 2 {
        return Err("至少选择 2 个字幕段".to_string());
    }

    let ids_str = segment_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM subtitle_segments WHERE id IN ({}) ORDER BY start_ms ASC",
                ids_str
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Err("未找到指定的字幕段".to_string());
    }

    // Collect info from all segments
    let first = &rows[0];
    let video_id: i64 = first.try_get("", "video_id").unwrap_or(0);
    let language: String = first.try_get("", "language").unwrap_or("zh".to_string());
    let min_start: i64 = first.try_get("", "start_ms").unwrap_or(0);

    let last = &rows[rows.len() - 1];
    let max_end: i64 = last.try_get("", "end_ms").unwrap_or(0);

    let merged_text: String = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "text").ok())
        .collect::<Vec<_>>()
        .join("");

    // Delete originals
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM subtitle_segments WHERE id IN ({})", ids_str),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Insert merged
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO subtitle_segments (video_id, language, start_ms, end_ms, text, source) \
             VALUES ({}, '{}', {}, {}, '{}', 'manual')",
            video_id,
            language,
            min_start,
            max_end,
            merged_text.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let new_id: i64 = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT last_insert_rowid() as id".to_string(),
        ),
    )
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get("", "id").ok())
    .unwrap_or(0);

    Ok(SubtitleSegment {
        id: new_id,
        video_id,
        language,
        start_ms: min_start,
        end_ms: max_end,
        text: merged_text,
        source: "manual".to_string(),
    })
}

/// Split a subtitle segment at a given time point
#[tauri::command]
pub async fn split_subtitle(
    state: State<'_, AppState>,
    segment_id: i64,
    split_at_ms: i64,
) -> Result<(SubtitleSegment, SubtitleSegment), String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT * FROM subtitle_segments WHERE id = {}", segment_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("字幕段不存在".to_string())?;

    let video_id: i64 = row.try_get("", "video_id").unwrap_or(0);
    let language: String = row.try_get("", "language").unwrap_or("zh".to_string());
    let start_ms: i64 = row.try_get("", "start_ms").unwrap_or(0);
    let end_ms: i64 = row.try_get("", "end_ms").unwrap_or(0);
    let text: String = row.try_get("", "text").unwrap_or_default();

    if split_at_ms <= start_ms || split_at_ms >= end_ms {
        return Err("拆分时间点必须在字幕段时间范围内".to_string());
    }

    // Split text roughly by ratio
    let ratio = (split_at_ms - start_ms) as f64 / (end_ms - start_ms) as f64;
    let char_count = text.chars().count();
    let split_pos = ((char_count as f64) * ratio).round() as usize;
    let text1: String = text.chars().take(split_pos).collect();
    let text2: String = text.chars().skip(split_pos).collect();

    // Delete original
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM subtitle_segments WHERE id = {}", segment_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Insert two new segments
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO subtitle_segments (video_id, language, start_ms, end_ms, text, source) \
             VALUES ({}, '{}', {}, {}, '{}', 'manual')",
            video_id, language, start_ms, split_at_ms, text1.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let id1: i64 = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT last_insert_rowid() as id".to_string(),
        ),
    )
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get("", "id").ok())
    .unwrap_or(0);

    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO subtitle_segments (video_id, language, start_ms, end_ms, text, source) \
             VALUES ({}, '{}', {}, {}, '{}', 'manual')",
            video_id, language, split_at_ms, end_ms, text2.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let id2: i64 = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT last_insert_rowid() as id".to_string(),
        ),
    )
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get("", "id").ok())
    .unwrap_or(0);

    Ok((
        SubtitleSegment {
            id: id1,
            video_id,
            language: language.clone(),
            start_ms,
            end_ms: split_at_ms,
            text: text1,
            source: "manual".to_string(),
        },
        SubtitleSegment {
            id: id2,
            video_id,
            language,
            start_ms: split_at_ms,
            end_ms,
            text: text2,
            source: "manual".to_string(),
        },
    ))
}

// ==================== Subtitle Export ====================

/// Export subtitles as SRT format
#[tauri::command]
pub async fn export_subtitles_srt(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<String, String> {
    let segments = service::list_subtitles(&state.db, video_id).await?;
    let base_ms = crate::core::subtitle::get_base_ms(&state.db, video_id).await;
    Ok(to_srt(&segments, base_ms))
}

/// Export subtitles as ASS format
#[tauri::command]
pub async fn export_subtitles_ass(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<String, String> {
    let segments = service::list_subtitles(&state.db, video_id).await?;
    let base_ms = crate::core::subtitle::get_base_ms(&state.db, video_id).await;
    Ok(crate::core::subtitle::generate_ass(&segments, base_ms))
}

/// Export subtitles as VTT format
#[tauri::command]
pub async fn export_subtitles_vtt(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<String, String> {
    let segments = service::list_subtitles(&state.db, video_id).await?;
    let base_ms = crate::core::subtitle::get_base_ms(&state.db, video_id).await;
    Ok(to_vtt(&segments, base_ms))
}

fn format_srt_time(ms: i64) -> String {
    let total_secs = ms / 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    let ms_part = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms_part)
}

fn format_vtt_time(ms: i64) -> String {
    let total_secs = ms / 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    let ms_part = ms % 1000;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms_part)
}

fn to_srt(segments: &[SubtitleSegment], base_ms: i64) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let start = seg.start_ms - base_ms;
        let end = seg.end_ms - base_ms;
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_srt_time(start.max(0)),
            format_srt_time(end.max(0)),
            seg.text,
        ));
    }
    out
}

fn to_vtt(segments: &[SubtitleSegment], base_ms: i64) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for seg in segments {
        let start = seg.start_ms - base_ms;
        let end = seg.end_ms - base_ms;
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_vtt_time(start.max(0)),
            format_vtt_time(end.max(0)),
            seg.text,
        ));
    }
    out
}

// ==================== ASR Service Management ====================

/// Validate that a directory contains a valid qwen3-asr-service installation
#[tauri::command]
pub async fn validate_asr_path(path: String) -> Result<ASRPathValidation, String> {
    Ok(ASRServiceManager::validate_path(Path::new(&path)))
}

/// Start the managed local ASR service using configured parameters.
///
/// Dispatches based on `asr_launch_mode` setting:
/// - `"native"` (default): runs the local setup.sh/start.sh script
/// - `"docker"`: runs the official docker container
#[tauri::command]
pub async fn start_asr_service(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let launch_mode_str = read_setting(&state, "asr_launch_mode")
        .await
        .unwrap_or_else(|| "native".to_string());

    let port: u16 = read_setting(&state, "asr_port")
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(8765);

    // Resolve launch_mode
    let launch_mode = if launch_mode_str == "docker" {
        let image = read_setting(&state, "asr_docker_image")
            .await
            .unwrap_or_default();
        if image.is_empty() {
            return Err("请先在设置中选择 Docker 镜像".to_string());
        }
        let data_dir_str = read_setting(&state, "asr_docker_data_dir").await;
        let data_dir = match data_dir_str {
            Some(s) if !s.is_empty() => std::path::PathBuf::from(s),
            _ => state.config_dir.clone(),
        };

        // Decide --gpus / --platform
        let cap = docker::detect_docker().await;
        let (use_gpu, force_platform) = decide_docker_runtime_flags(&image, &cap);

        ASRLaunchMode::Docker {
            image,
            data_dir,
            use_gpu,
            force_platform,
        }
    } else {
        let base_path = read_setting(&state, "asr_local_path")
            .await
            .ok_or("请先在设置中配置 ASR 服务路径")?;
        if base_path.is_empty() {
            return Err("请先在设置中配置 ASR 服务路径".to_string());
        }
        ASRLaunchMode::Native {
            base_dir: std::path::PathBuf::from(base_path),
        }
    };

    let config = ASRStartConfig {
        launch_mode,
        port,
        device: read_setting(&state, "asr_local_device")
            .await
            .unwrap_or_else(|| "auto".to_string()),
        model_size: read_setting(&state, "asr_local_model_size")
            .await
            .unwrap_or_else(|| "auto".to_string()),
        enable_align: read_setting(&state, "asr_local_enable_align")
            .await
            .map(|v| v != "false")
            .unwrap_or(true),
        enable_punc: read_setting(&state, "asr_local_enable_punc")
            .await
            .map(|v| v == "true")
            .unwrap_or(true),
        model_source: read_setting(&state, "asr_local_model_source")
            .await
            .unwrap_or_else(|| "modelscope".to_string()),
        max_segment: read_setting(&state, "asr_local_max_segment")
            .await
            .and_then(|v| v.parse().ok())
            .unwrap_or(5),
        host: "127.0.0.1".to_string(),
    };

    state
        .asr_service_manager
        .start_service(config, app_handle)
        .await
}

/// Decide (use_gpu, force_platform) from the image tag and host capability.
///
/// Rules:
/// - Image tag contains "-cpu" and host is arm64 → force platform linux/amd64
/// - Image tag contains "-arm64" → no flags
/// - Otherwise (bare `latest` / no suffix, i.e. CUDA build) → `--gpus all`
fn decide_docker_runtime_flags(
    image: &str,
    cap: &DockerCapability,
) -> (bool, Option<String>) {
    let tag = image.rsplit(':').next().unwrap_or("");
    if tag.contains("-cpu") {
        let force = if cap.host_arch == "arm64" {
            Some("linux/amd64".to_string())
        } else {
            None
        };
        (false, force)
    } else if tag.contains("-arm64") {
        (false, None)
    } else {
        // Bare `latest` / `1.1.0` → CUDA build
        (true, None)
    }
}

/// Stop the managed local ASR service
#[tauri::command]
pub async fn stop_asr_service(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    // For external terminal mode, stop only updates status.
    // The user should close the terminal window manually.
    if let Err(e) = state.asr_service_manager.stop().await {
        tracing::warn!("ASR stop error (external mode): {}", e);
    }

    // Mark any pending/processing ASR tasks as failed since the service is now stopped
    if let Err(e) = sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        "UPDATE asr_tasks SET status = 'failed', error_message = 'ASR 服务已停止' \
         WHERE status IN ('processing', 'pending')",
    )
    .await
    {
        tracing::warn!("Failed to mark pending ASR tasks as failed: {}", e);
    }

    let _ = app_handle.emit(
        "asr-service-status",
        state.asr_service_manager.status_info(),
    );
    Ok(())
}

// ==================== Docker mode commands ====================

/// Probe whether Docker CLI and daemon are available on this host
#[tauri::command]
pub async fn check_docker_capability() -> Result<DockerCapability, String> {
    Ok(docker::detect_docker().await)
}

/// Check whether a given docker image has been pulled locally
#[tauri::command]
pub async fn check_docker_image_pulled(image: String) -> Result<bool, String> {
    Ok(docker::check_image_pulled(&image).await)
}

/// Open the system terminal and run `docker pull <image>`
#[tauri::command]
pub async fn open_docker_pull_terminal(image: String) -> Result<(), String> {
    docker::open_pull_terminal(&image)
}

/// Force-remove a leftover ASR docker container (used by the conflict dialog)
#[tauri::command]
pub async fn force_remove_asr_container() -> Result<(), String> {
    ASRServiceManager::force_remove_docker_container().await
}

/// Returns the default data directory for docker mode (app data dir).
/// The mount target is `{dir}/models` → `/app/models`.
#[tauri::command]
pub fn get_default_asr_docker_data_dir(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.config_dir.to_string_lossy().to_string())
}

/// Open an external terminal to run the ASR setup script
#[tauri::command]
pub async fn open_asr_setup_terminal(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let base_path = read_setting(&state, "asr_local_path")
        .await
        .ok_or("请先在设置中配置 ASR 服务路径")?;
    if base_path.is_empty() {
        return Err("请先在设置中配置 ASR 服务路径".to_string());
    }
    ASRServiceManager::open_setup_terminal(Path::new(&base_path))
}

/// Get current ASR service status
#[tauri::command]
pub fn get_asr_service_status(
    state: State<'_, AppState>,
) -> Result<ASRServiceStatusInfo, String> {
    Ok(state.asr_service_manager.status_info())
}

/// Get recent ASR service log lines
#[tauri::command]
pub fn get_asr_service_logs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    Ok(state.asr_service_manager.get_logs(limit.unwrap_or(200)))
}

// ==================== ASR Task Queue ====================

/// Submit an ASR task to the queue (enqueue for serial execution)
#[tauri::command]
pub async fn submit_asr_queued(
    state: State<'_, AppState>,
    video_id: i64,
    language: Option<String>,
) -> Result<i64, String> {
    // Resolve language: param > settings > default
    let lang = match language {
        Some(l) if !l.is_empty() => l,
        _ => read_setting(&state, "asr_language")
            .await
            .unwrap_or_else(|| "Chinese".to_string()),
    };

    state
        .asr_task_queue
        .enqueue(video_id, lang)
        .await
}

/// Cancel an ASR task (queued or running)
#[tauri::command]
pub fn cancel_asr_task(
    state: State<'_, AppState>,
    asr_task_id: i64,
) -> Result<bool, String> {
    state.asr_task_queue.cancel(asr_task_id)
}

/// Get current ASR queue snapshot (running + pending tasks)
#[tauri::command]
pub fn get_asr_queue_snapshot(
    state: State<'_, AppState>,
) -> Result<Vec<ASRQueueItem>, String> {
    Ok(state.asr_task_queue.get_queue_snapshot())
}

#[derive(Debug, serde::Serialize)]
pub struct RepairSubtitleTimestampsResult {
    /// 检查过的视频数
    pub videos_checked: i64,
    /// 确认受影响的视频数
    pub videos_fixed: i64,
    /// 被修正的字幕段数
    pub segments_fixed: i64,
}

/// 修复 commit `7828c79` 之前错误历法算法（y*365+mo*30）污染的 `subtitle_segments` 时间戳。
///
/// 判定逻辑：逐视频比较 start_ms 的中位区间是否明显偏离 Unix ms（用 1e13 作阈值），
/// 若是则用 `legacy_ms - unix_ms` 作为 delta 将该视频所有字幕段整体 shift 回 Unix ms。
#[tauri::command]
pub async fn repair_subtitle_timestamps(
    state: State<'_, AppState>,
) -> Result<RepairSubtitleTimestampsResult, String> {
    use crate::asr::service::{parse_recorded_at_legacy_buggy_ms, parse_recorded_at_to_unix_ms};

    // 超过此阈值（约对应 UTC 2286-11 以后）几乎可以认定不是合理 Unix ms，是旧算法遗留
    const BAD_THRESHOLD_MS: i64 = 10_000_000_000_000;

    // 找出存在"异常行"的所有 video_id，连带该视频的 recorded_at
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT DISTINCT s.video_id, v.recorded_at \
                 FROM subtitle_segments s \
                 JOIN videos v ON v.id = s.video_id \
                 WHERE s.start_ms > {}",
                BAD_THRESHOLD_MS
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut videos_fixed = 0i64;
    let mut segments_fixed = 0i64;

    for row in &rows {
        let video_id: i64 = row.try_get("", "video_id").unwrap_or(0);
        let recorded_at: Option<String> =
            row.try_get::<Option<String>>("", "recorded_at").unwrap_or(None);
        let recorded_at = match recorded_at {
            Some(s) if !s.is_empty() => s,
            _ => {
                tracing::warn!(
                    "[repair] video_id={} 缺少 recorded_at，跳过",
                    video_id
                );
                continue;
            }
        };

        let legacy = match parse_recorded_at_legacy_buggy_ms(&recorded_at) {
            Some(v) => v,
            None => {
                tracing::warn!(
                    "[repair] 无法用旧算法解析 recorded_at={:?}，跳过 video_id={}",
                    recorded_at,
                    video_id
                );
                continue;
            }
        };
        let unix = match parse_recorded_at_to_unix_ms(&recorded_at) {
            Some(v) => v,
            None => {
                tracing::warn!(
                    "[repair] 无法用 chrono 解析 recorded_at={:?}，跳过 video_id={}",
                    recorded_at,
                    video_id
                );
                continue;
            }
        };
        let delta = legacy - unix; // legacy 偏大，减掉它回到 Unix ms

        let affected = sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!(
                "UPDATE subtitle_segments \
                 SET start_ms = start_ms - {}, end_ms = end_ms - {} \
                 WHERE video_id = {} AND start_ms > {}",
                delta, delta, video_id, BAD_THRESHOLD_MS
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        let n = affected.rows_affected() as i64;
        if n > 0 {
            videos_fixed += 1;
            segments_fixed += n;
            tracing::info!(
                "[repair] video_id={} recorded_at={} delta={} 修正 {} 段字幕",
                video_id,
                recorded_at,
                delta,
                n
            );
        }
    }

    Ok(RepairSubtitleTimestampsResult {
        videos_checked: rows.len() as i64,
        videos_fixed,
        segments_fixed,
    })
}
