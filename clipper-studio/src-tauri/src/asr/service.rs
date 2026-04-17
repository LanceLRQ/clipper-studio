use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::db::Database;

use super::provider::{ASRProvider, ASRSegment, ASRTaskStatus};
use super::splitter;

/// Maximum automatic retry count
const MAX_AUTO_RETRIES: u32 = 2;
/// Initial retry delay in seconds
const INITIAL_RETRY_DELAY_SECS: u64 = 5;

/// Subtitle segment (stored with absolute time)
#[derive(Debug, Clone, Serialize)]
pub struct SubtitleSegment {
    pub id: i64,
    pub video_id: i64,
    pub language: String,
    /// Absolute time (Unix milliseconds) or file-relative milliseconds if no recorded_at
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub source: String,
}

/// ASR task info for frontend display
#[derive(Debug, Clone, Serialize)]
pub struct ASRTaskInfo {
    pub id: i64,
    pub video_id: i64,
    pub status: String,
    pub progress: f64,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub segment_count: Option<i32>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Convert video/audio to 16kHz mono WAV for ASR processing.
///
/// Uses FFmpeg: `ffmpeg -i input -ar 16000 -ac 1 -c:a pcm_s16le output.wav`
/// Returns the path to the temporary WAV file.
pub async fn convert_to_asr_wav(
    ffmpeg_path: &str,
    input: &Path,
    cancel_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> Result<PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let wav_path = temp_dir.join(format!("clipper_asr_{}.wav", timestamp));

    tracing::info!(
        "Converting audio for ASR: {} -> {}",
        input.display(),
        wav_path.display()
    );

    let mut child = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-i",
            &input.to_string_lossy(),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            "-y",
            &wav_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("音频转换失败: {}", e))?;

    // Poll child process with cancellation check
    loop {
        // Check cancellation
        if let Some(ref flag) = cancel_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = tokio::fs::remove_file(&wav_path).await;
                return Err("Task cancelled".to_string());
            }
        }

        // Check if process has finished (non-blocking)
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let _ = tokio::fs::remove_file(&wav_path).await;
                    return Err(format!("音频转换失败: FFmpeg exited with {}", status));
                }
                return Ok(wav_path);
            }
            Ok(None) => {
                // Still running, wait a bit before next check
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&wav_path).await;
                return Err(format!("音频转换失败: {}", e));
            }
        }
    }
}

/// Submit an ASR task for a video
pub async fn submit_asr(
    db: &Database,
    provider: &Arc<dyn ASRProvider>,
    ffmpeg_path: &str,
    video_id: i64,
    language: Option<&str>,
    force: bool,
) -> Result<i64, String> {
    // Check if video has existing subtitles
    if !force {
        let existing = sea_orm::ConnectionTrait::query_one(
            db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT COUNT(*) as cnt FROM subtitle_segments WHERE video_id = {}",
                    video_id
                ),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0);

        if existing > 0 {
            return Err("该视频已有字幕，使用 force=true 覆盖".to_string());
        }
    }

    // Get video file path
    let video_row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT file_path FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let file_path: String = video_row.try_get("", "file_path").unwrap_or_default();
    let lang = language.unwrap_or("Chinese");

    // Convert to 16kHz mono WAV for ASR
    let wav_path = convert_to_asr_wav(ffmpeg_path, Path::new(&file_path), None).await?;

    // Submit to ASR provider (use WAV file)
    let remote_task_id = match provider
        .submit(&wav_path, Some(lang))
        .await
    {
        Ok(id) => {
            // Clean up temp WAV after successful submit
            let _ = tokio::fs::remove_file(&wav_path).await;
            id
        }
        Err(e) => {
            // Clean up temp WAV on error too
            let _ = tokio::fs::remove_file(&wav_path).await;
            return Err(e);
        }
    };

    // Create asr_task record
    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "INSERT INTO asr_tasks (video_id, status, asr_provider_id, remote_task_id, language, started_at) \
             VALUES ({}, 'processing', '{}', '{}', '{}', datetime('now'))",
            video_id,
            provider.provider_id(),
            remote_task_id.replace('\'', "''"),
            lang,
        ),
    )
    .await
    .map_err(|e| format!("创建 ASR 任务失败: {}", e))?;

    let task_id: i64 = sea_orm::ConnectionTrait::query_one(
        db.conn(),
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

    tracing::info!(
        "ASR task {} created for video {} (remote: {})",
        task_id,
        video_id,
        remote_task_id
    );

    Ok(task_id)
}

/// Poll ASR task status; if completed, import segments into DB
pub async fn poll_asr(
    db: &Database,
    provider: &Arc<dyn ASRProvider>,
    asr_task_id: i64,
) -> Result<ASRTaskInfo, String> {
    // Get task info
    let task_row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT * FROM asr_tasks WHERE id = {}", asr_task_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("ASR 任务不存在".to_string())?;

    let remote_task_id: String = task_row.try_get("", "remote_task_id").unwrap_or_default();
    let video_id: i64 = task_row.try_get("", "video_id").unwrap_or(0);
    let current_status: String = task_row.try_get("", "status").unwrap_or_default();
    let retry_count: i32 = task_row.try_get("", "retry_count").unwrap_or(0);

    // Already completed/failed? Return current state
    if current_status == "completed" || current_status == "failed" {
        return Ok(row_to_task_info(&task_row));
    }

    // Query remote status
    let asr_status = provider.query(&remote_task_id).await?;

    match asr_status {
        ASRTaskStatus::Pending => {
            // Still pending, no update needed
        }
        ASRTaskStatus::Processing { progress } => {
            let _ = sea_orm::ConnectionTrait::execute_unprepared(
                db.conn(),
                &format!(
                    "UPDATE asr_tasks SET status = 'processing', progress = {} WHERE id = {}",
                    progress, asr_task_id
                ),
            )
            .await;
        }
        ASRTaskStatus::Completed { segments: raw_segments } => {
            // Read max_chars setting for subtitle splitting
            let max_chars = read_setting_from_db(db, "asr_max_chars")
                .await
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(15);

            // Split raw segments by punctuation and character count
            let segments = splitter::split_segments(&raw_segments, max_chars);

            // Import segments into subtitle_segments
            let language: String = task_row.try_get("", "language").unwrap_or("Chinese".to_string());
            let count = import_segments(db, video_id, &language, &segments).await?;

            let _ = sea_orm::ConnectionTrait::execute_unprepared(
                db.conn(),
                &format!(
                    "UPDATE asr_tasks SET status = 'completed', progress = 1.0, \
                     segment_count = {}, completed_at = datetime('now') WHERE id = {}",
                    count, asr_task_id
                ),
            )
            .await;

            // Update video has_subtitle flag
            let _ = sea_orm::ConnectionTrait::execute_unprepared(
                db.conn(),
                &format!(
                    "UPDATE videos SET has_subtitle = 1 WHERE id = {}",
                    video_id
                ),
            )
            .await;

            tracing::info!(
                "ASR task {} completed: {} segments imported",
                asr_task_id,
                count
            );
        }
        ASRTaskStatus::RetryableError { error, .. } => {
            if retry_count < MAX_AUTO_RETRIES as i32 {
                // Schedule retry
                let _ = sea_orm::ConnectionTrait::execute_unprepared(
                    db.conn(),
                    &format!(
                        "UPDATE asr_tasks SET retry_count = {}, \
                         error_message = '{}' WHERE id = {}",
                        retry_count + 1,
                        error.replace('\'', "''"),
                        asr_task_id
                    ),
                )
                .await;

                let delay = INITIAL_RETRY_DELAY_SECS * 2u64.pow(retry_count as u32);
                tracing::warn!(
                    "ASR task {} retryable error (attempt {}): {}. Retry in {}s",
                    asr_task_id,
                    retry_count + 1,
                    error,
                    delay
                );
            } else {
                // Max retries exceeded
                let _ = sea_orm::ConnectionTrait::execute_unprepared(
                    db.conn(),
                    &format!(
                        "UPDATE asr_tasks SET status = 'failed', \
                         error_message = '{}' WHERE id = {}",
                        error.replace('\'', "''"),
                        asr_task_id
                    ),
                )
                .await;
            }
        }
        ASRTaskStatus::PermanentError { error } => {
            let _ = sea_orm::ConnectionTrait::execute_unprepared(
                db.conn(),
                &format!(
                    "UPDATE asr_tasks SET status = 'failed', \
                     error_message = '{}' WHERE id = {}",
                    error.replace('\'', "''"),
                    asr_task_id
                ),
            )
            .await;
        }
    }

    // Re-fetch updated task
    let updated_row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT * FROM asr_tasks WHERE id = {}", asr_task_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("ASR 任务不存在".to_string())?;

    Ok(row_to_task_info(&updated_row))
}

/// Import ASR segments into subtitle_segments table.
///
/// Converts file-relative time (seconds) to absolute time (Unix ms)
/// using the video's `recorded_at` timestamp.
pub async fn import_segments(
    db: &Database,
    video_id: i64,
    language: &str,
    segments: &[ASRSegment],
) -> Result<usize, String> {
    if segments.is_empty() {
        return Ok(0);
    }

    // Get video's recorded_at for absolute time conversion
    let video_row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT recorded_at FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .ok()
    .flatten();

    let base_ms: i64 = video_row
        .and_then(|r| r.try_get::<String>("", "recorded_at").ok())
        .and_then(|ts| parse_recorded_at_to_unix_ms(&ts))
        .unwrap_or(0); // If no recorded_at, use file-relative time (base = 0)

    // Use a transaction for atomic delete + batch insert (10x faster on SQLite)
    let _ = sea_orm::ConnectionTrait::execute_unprepared(db.conn(), "BEGIN").await;

    // Delete existing subtitles for this video (force mode)
    let _ = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "DELETE FROM subtitle_segments WHERE video_id = {} AND source = 'asr'",
            video_id
        ),
    )
    .await;

    // Batch insert segments
    let mut count = 0;
    for seg in segments {
        let start_ms = base_ms + (seg.start * 1000.0) as i64;
        let end_ms = base_ms + (seg.end * 1000.0) as i64;
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }

        let result = sea_orm::ConnectionTrait::execute_unprepared(
            db.conn(),
            &format!(
                "INSERT INTO subtitle_segments (video_id, language, start_ms, end_ms, text, source) \
                 VALUES ({}, '{}', {}, {}, '{}', 'asr')",
                video_id,
                language,
                start_ms,
                end_ms,
                text.replace('\'', "''"),
            ),
        )
        .await;

        if result.is_ok() {
            count += 1;
        }
    }

    let _ = sea_orm::ConnectionTrait::execute_unprepared(db.conn(), "COMMIT").await;

    Ok(count)
}

/// Read a setting value from settings_kv table.
/// 命中 `is_secret_key` 的 key 会透明 base64 解码（兼容旧明文）。
pub async fn read_setting_from_db(db: &Database, key: &str) -> Option<String> {
    let raw = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT value FROM settings_kv WHERE key = '{}'", key),
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

/// Parse "yyyy-MM-dd HH:mm:ss" to Unix milliseconds (UTC)
pub fn parse_recorded_at_to_unix_ms(ts: &str) -> Option<i64> {
    use chrono::NaiveDateTime;
    let ndt = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(ndt.and_utc().timestamp_millis())
}

/// List subtitle segments for a video
pub async fn list_subtitles(
    db: &Database,
    video_id: i64,
) -> Result<Vec<SubtitleSegment>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM subtitle_segments WHERE video_id = {} ORDER BY start_ms ASC",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(row_to_segment).collect())
}

/// Search subtitles by text (FTS5)
pub async fn search_subtitles(
    db: &Database,
    query: &str,
    video_id: Option<i64>,
) -> Result<Vec<SubtitleSegment>, String> {
    let sql = match video_id {
        Some(vid) => format!(
            "SELECT s.* FROM subtitle_segments s \
             INNER JOIN subtitle_fts fts ON s.id = fts.rowid \
             WHERE fts.text MATCH '{}' AND s.video_id = {} \
             ORDER BY s.start_ms ASC",
            query.replace('\'', "''"),
            vid
        ),
        None => format!(
            "SELECT s.* FROM subtitle_segments s \
             INNER JOIN subtitle_fts fts ON s.id = fts.rowid \
             WHERE fts.text MATCH '{}' \
             ORDER BY s.start_ms ASC",
            query.replace('\'', "''"),
        ),
    };

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(row_to_segment).collect())
}

/// Global subtitle search result with video metadata
#[derive(Debug, Clone, Serialize)]
pub struct SubtitleSearchResult {
    pub id: i64,
    pub video_id: i64,
    pub language: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub source: String,
    pub video_file_name: String,
    pub video_duration_ms: Option<i64>,
    pub streamer_name: Option<String>,
    pub stream_title: Option<String>,
    pub recorded_at: Option<String>,
}

/// Sanitize user input for FTS5 MATCH query
fn sanitize_fts5_query(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Escape double quotes and wrap in quotes for phrase search
    let escaped = trimmed.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

/// Search subtitles globally with video metadata (FTS5)
pub async fn search_subtitles_global(
    db: &Database,
    query: &str,
) -> Result<Vec<SubtitleSearchResult>, String> {
    let safe_query = sanitize_fts5_query(query);
    if safe_query.is_empty() {
        return Ok(vec![]);
    }

    let sql = format!(
        "SELECT s.id, s.video_id, s.language, s.start_ms, s.end_ms, s.text, s.source, \
         v.file_name AS video_file_name, v.duration_ms AS video_duration_ms, \
         st.name AS streamer_name, v.stream_title, v.recorded_at \
         FROM subtitle_segments s \
         INNER JOIN subtitle_fts fts ON s.id = fts.rowid \
         LEFT JOIN videos v ON s.video_id = v.id \
         LEFT JOIN streamers st ON v.streamer_id = st.id \
         WHERE fts.text MATCH '{}' \
         ORDER BY v.id, s.start_ms ASC \
         LIMIT 200",
        safe_query.replace('\'', "''"),
    );

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .iter()
        .map(|row| SubtitleSearchResult {
            id: row.try_get("", "id").unwrap_or(0),
            video_id: row.try_get("", "video_id").unwrap_or(0),
            language: row.try_get("", "language").unwrap_or_default(),
            start_ms: row.try_get("", "start_ms").unwrap_or(0),
            end_ms: row.try_get("", "end_ms").unwrap_or(0),
            text: row.try_get("", "text").unwrap_or_default(),
            source: row.try_get("", "source").unwrap_or_default(),
            video_file_name: row.try_get("", "video_file_name").unwrap_or_default(),
            video_duration_ms: row.try_get("", "video_duration_ms").ok(),
            streamer_name: row.try_get("", "streamer_name").ok(),
            stream_title: row.try_get("", "stream_title").ok(),
            recorded_at: row.try_get("", "recorded_at").ok(),
        })
        .collect())
}

/// List ASR tasks for a video
pub async fn list_asr_tasks(
    db: &Database,
    video_id: Option<i64>,
) -> Result<Vec<ASRTaskInfo>, String> {
    let where_clause = video_id
        .map(|id| format!("WHERE video_id = {}", id))
        .unwrap_or_default();

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM asr_tasks {} ORDER BY created_at DESC",
                where_clause
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(row_to_task_info).collect())
}

fn row_to_segment(row: &sea_orm::QueryResult) -> SubtitleSegment {
    SubtitleSegment {
        id: row.try_get("", "id").unwrap_or(0),
        video_id: row.try_get("", "video_id").unwrap_or(0),
        language: row.try_get("", "language").unwrap_or("Chinese".to_string()),
        start_ms: row.try_get("", "start_ms").unwrap_or(0),
        end_ms: row.try_get("", "end_ms").unwrap_or(0),
        text: row.try_get("", "text").unwrap_or_default(),
        source: row.try_get("", "source").unwrap_or("asr".to_string()),
    }
}

fn row_to_task_info(row: &sea_orm::QueryResult) -> ASRTaskInfo {
    ASRTaskInfo {
        id: row.try_get("", "id").unwrap_or(0),
        video_id: row.try_get("", "video_id").unwrap_or(0),
        status: row.try_get("", "status").unwrap_or_default(),
        progress: row.try_get("", "progress").unwrap_or(0.0),
        error_message: row.try_get("", "error_message").ok(),
        retry_count: row.try_get("", "retry_count").unwrap_or(0),
        segment_count: row.try_get("", "segment_count").ok(),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
        completed_at: row.try_get("", "completed_at").ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== parse_recorded_at_to_unix_ms =====

    #[test]
    fn test_parse_recorded_at_standard() {
        use chrono::NaiveDateTime;
        let result = parse_recorded_at_to_unix_ms("2026-04-05 20:30:00");
        assert!(result.is_some());
        let expected = NaiveDateTime::parse_from_str("2026-04-05 20:30:00", "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_parse_recorded_at_midnight() {
        use chrono::NaiveDateTime;
        let result = parse_recorded_at_to_unix_ms("2026-01-01 00:00:00");
        assert!(result.is_some());
        let expected = NaiveDateTime::parse_from_str("2026-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_parse_recorded_at_invalid() {
        assert!(parse_recorded_at_to_unix_ms("").is_none());
        assert!(parse_recorded_at_to_unix_ms("invalid").is_none());
    }

    #[test]
    fn test_parse_recorded_at_incomplete() {
        // Missing time portion → only 3 parts
        assert!(parse_recorded_at_to_unix_ms("2026-04-05").is_none());
        // Only hours and minutes → 5 parts
        assert!(parse_recorded_at_to_unix_ms("2026-04-05 20:30").is_none());
    }

    #[test]
    fn test_parse_recorded_at_non_numeric() {
        assert!(parse_recorded_at_to_unix_ms("abcd-ef-gh ij:kl:mn").is_none());
    }

    #[test]
    fn test_parse_recorded_at_consistency() {
        // Two timestamps: later one should yield a larger value
        let t1 = parse_recorded_at_to_unix_ms("2026-04-05 20:00:00").unwrap();
        let t2 = parse_recorded_at_to_unix_ms("2026-04-05 21:00:00").unwrap();
        assert!(t2 > t1);
        // Difference should be approximately 1 hour
        assert_eq!(t2 - t1, 3600 * 1000);
    }
}
