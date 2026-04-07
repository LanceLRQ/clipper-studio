use std::sync::Arc;

use tauri::State;

use crate::asr::local::LocalASRProvider;
use crate::asr::provider::{ASRHealthInfo, ASRProvider};
use crate::asr::remote::RemoteASRProvider;
use crate::asr::service::{self, ASRTaskInfo, SubtitleSegment};
use crate::AppState;

/// Helper: read a setting from settings_kv
async fn read_setting(state: &AppState, key: &str) -> Option<String> {
    sea_orm::ConnectionTrait::query_one(
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
    .and_then(|r| r.try_get::<String>("", "value").ok())
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

    match mode.as_str() {
        "disabled" => Err("ASR 功能已禁用，请在设置中启用".to_string()),
        "remote" => {
            let url = read_setting(state, "asr_url")
                .await
                .ok_or("请先在设置中配置远程 ASR 地址")?;
            let api_key = read_setting(state, "asr_api_key").await;
            Ok(Arc::new(RemoteASRProvider::new(&url, api_key)))
        }
        _ => {
            // "local" or default
            let port: u16 = read_setting(state, "asr_port")
                .await
                .and_then(|v| v.parse().ok())
                .unwrap_or(8765);
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
    service::submit_asr(
        &state.db,
        &provider,
        video_id,
        language.as_deref(),
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

/// List subtitle segments for a video
#[tauri::command]
pub async fn list_subtitles(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<Vec<SubtitleSegment>, String> {
    service::list_subtitles(&state.db, video_id).await
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

/// Check ASR engine health
#[tauri::command]
pub async fn check_asr_health(
    state: State<'_, AppState>,
) -> Result<ASRHealthInfo, String> {
    let provider = get_provider(&state).await?;
    provider.health().await
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
