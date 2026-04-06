use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use crate::utils::ffmpeg;
use crate::utils::hash;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VideoInfo {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub file_hash: Option<String>,
    pub file_size: i64,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format_name: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub has_subtitle: bool,
    pub has_danmaku: bool,
    pub has_envelope: bool,
    pub workspace_id: Option<i64>,
    pub session_id: Option<i64>,
    pub streamer_id: Option<i64>,
    pub streamer_name: Option<String>,
    pub stream_title: Option<String>,
    pub recorded_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportVideoRequest {
    pub file_path: String,
    pub workspace_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListVideosRequest {
    pub workspace_id: Option<i64>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListVideosResponse {
    pub videos: Vec<VideoInfo>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

/// Import a video file: probe metadata → hash → insert into database
#[tauri::command]
pub async fn import_video(
    state: State<'_, AppState>,
    req: ImportVideoRequest,
) -> Result<VideoInfo, String> {
    let path = std::path::Path::new(&req.file_path);

    // Validate file exists
    if !path.exists() {
        return Err(format!("文件不存在: {}", req.file_path));
    }
    if !path.is_file() {
        return Err("路径不是一个文件".to_string());
    }

    // Check if already imported (by path)
    let existing = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id FROM videos WHERE file_path = '{}'",
                req.file_path.replace('\'', "''")
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    if existing.is_some() {
        return Err("该文件已导入".to_string());
    }

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Get file size
    let metadata = std::fs::metadata(path).map_err(|e| format!("无法读取文件信息: {}", e))?;
    let file_size = metadata.len() as i64;

    // FFprobe: extract metadata
    let probe_result = if !state.ffprobe_path.is_empty() {
        match ffmpeg::probe(&state.ffprobe_path, path) {
            Ok(result) => Some(result),
            Err(e) => {
                tracing::warn!("FFprobe failed for {}: {}", req.file_path, e);
                None
            }
        }
    } else {
        tracing::warn!("FFprobe not available, skipping metadata extraction");
        None
    };

    // Blake3 hash (async)
    let file_hash = match hash::blake3_file(path).await {
        Ok(h) => Some(h),
        Err(e) => {
            tracing::warn!("Blake3 hash failed for {}: {}", req.file_path, e);
            None
        }
    };

    // Check duplicate by hash
    if let Some(ref fh) = file_hash {
        let dup = sea_orm::ConnectionTrait::query_one(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!("SELECT id, file_path FROM videos WHERE file_hash = '{}'", fh),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        if let Some(dup_row) = dup {
            let dup_path: String = dup_row.try_get("", "file_path").unwrap_or_default();
            return Err(format!("文件已存在（重复文件: {}）", dup_path));
        }
    }

    // Insert into database
    let duration_ms = probe_result.as_ref().and_then(|p| p.duration_ms);
    let width = probe_result.as_ref().and_then(|p| p.width);
    let height = probe_result.as_ref().and_then(|p| p.height);
    let _format_name = probe_result.as_ref().and_then(|p| p.format_name.clone());
    let video_codec = probe_result.as_ref().and_then(|p| p.video_codec.clone());
    let _audio_codec = probe_result.as_ref().and_then(|p| p.audio_codec.clone());

    let ws_id_sql = req.workspace_id.map(|id| id.to_string()).unwrap_or("NULL".to_string());
    let hash_sql = file_hash.as_deref().map(|h| format!("'{}'", h)).unwrap_or("NULL".to_string());
    let dur_sql = duration_ms.map(|d| d.to_string()).unwrap_or("NULL".to_string());
    let w_sql = width.map(|w| w.to_string()).unwrap_or("NULL".to_string());
    let h_sql = height.map(|h| h.to_string()).unwrap_or("NULL".to_string());

    let sql = format!(
        "INSERT INTO videos (file_path, file_name, file_hash, file_size, duration_ms, width, height, workspace_id) \
         VALUES ('{}', '{}', {}, {}, {}, {}, {}, {})",
        req.file_path.replace('\'', "''"),
        file_name.replace('\'', "''"),
        hash_sql,
        file_size,
        dur_sql,
        w_sql,
        h_sql,
        ws_id_sql,
    );

    sea_orm::ConnectionTrait::execute_unprepared(state.db.conn(), &sql)
        .await
        .map_err(|e| format!("数据库写入失败: {}", e))?;

    // Query the inserted row
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.*, st.name as streamer_name FROM videos v \
                 LEFT JOIN streamers st ON v.streamer_id = st.id \
                 WHERE v.file_path = '{}'",
                req.file_path.replace('\'', "''")
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("导入后查询失败".to_string())?;

    let video = row_to_video_info(&row);

    tracing::info!(
        "Video imported: {} ({}x{}, {}ms, codec: {:?})",
        file_name,
        video.width.unwrap_or(0),
        video.height.unwrap_or(0),
        video.duration_ms.unwrap_or(0),
        video_codec,
    );

    Ok(video)
}

/// List videos with optional workspace filter and pagination
#[tauri::command]
pub async fn list_videos(
    state: State<'_, AppState>,
    req: ListVideosRequest,
) -> Result<ListVideosResponse, String> {
    let page = req.page.unwrap_or(1).max(1);
    let page_size = req.page_size.unwrap_or(50).min(200);
    let offset = (page - 1) * page_size;

    let where_clause = match req.workspace_id {
        Some(id) => format!("WHERE v.workspace_id = {}", id),
        None => String::new(),
    };

    // Count total
    let count_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT COUNT(*) as cnt FROM videos v {}", where_clause),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let total: i64 = count_row
        .and_then(|r| r.try_get("", "cnt").ok())
        .unwrap_or(0);

    // Query page
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.*, st.name as streamer_name FROM videos v \
                 LEFT JOIN streamers st ON v.streamer_id = st.id \
                 {} ORDER BY v.created_at DESC LIMIT {} OFFSET {}",
                where_clause, page_size, offset
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let videos: Vec<VideoInfo> = rows.iter().map(row_to_video_info).collect();

    Ok(ListVideosResponse {
        videos,
        total,
        page,
        page_size,
    })
}

/// Get a single video by ID
#[tauri::command]
pub async fn get_video(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<VideoInfo, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT v.*, st.name as streamer_name FROM videos v \
                 LEFT JOIN streamers st ON v.streamer_id = st.id \
                 WHERE v.id = {}",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    Ok(row_to_video_info(&row))
}

/// Delete a video record (does not delete file on disk)
#[tauri::command]
pub async fn delete_video(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<(), String> {
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM videos WHERE id = {}", video_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Video deleted: id={}", video_id);
    Ok(())
}

// ==================== Session & Streamer queries ====================

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: i64,
    pub workspace_id: i64,
    pub streamer_id: Option<i64>,
    pub streamer_name: Option<String>,
    pub title: Option<String>,
    pub started_at: Option<String>,
    pub file_count: i32,
    pub videos: Vec<VideoInfo>,
}

#[derive(Debug, Serialize)]
pub struct StreamerInfo {
    pub id: i64,
    pub platform: String,
    pub room_id: Option<String>,
    pub name: String,
    pub video_count: i64,
}

/// List videos grouped by recording sessions
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
    workspace_id: Option<i64>,
) -> Result<Vec<SessionInfo>, String> {
    let ws_filter = workspace_id
        .map(|id| format!("WHERE s.workspace_id = {}", id))
        .unwrap_or_default();

    let session_rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT s.*, st.name as streamer_name FROM recording_sessions s \
                 LEFT JOIN streamers st ON s.streamer_id = st.id \
                 {} ORDER BY s.started_at DESC",
                ws_filter
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut sessions = Vec::new();

    for srow in &session_rows {
        let session_id: i64 = srow.try_get("", "id").unwrap_or(0);

        // Get videos in this session
        let video_rows = sea_orm::ConnectionTrait::query_all(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT v.*, st.name as streamer_name FROM videos v \
                     LEFT JOIN streamers st ON v.streamer_id = st.id \
                     WHERE v.session_id = {} ORDER BY v.recorded_at ASC",
                    session_id
                ),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        let videos: Vec<VideoInfo> = video_rows.iter().map(row_to_video_info).collect();

        sessions.push(SessionInfo {
            id: session_id,
            workspace_id: srow.try_get("", "workspace_id").unwrap_or(0),
            streamer_id: srow.try_get("", "streamer_id").ok(),
            streamer_name: srow.try_get("", "streamer_name").ok(),
            title: srow.try_get("", "title").ok(),
            started_at: srow.try_get("", "started_at").ok(),
            file_count: srow.try_get("", "file_count").unwrap_or(0),
            videos,
        });
    }

    Ok(sessions)
}

/// List all streamers
#[tauri::command]
pub async fn list_streamers(
    state: State<'_, AppState>,
) -> Result<Vec<StreamerInfo>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT st.*, COUNT(v.id) as video_count FROM streamers st \
             LEFT JOIN videos v ON st.id = v.streamer_id \
             GROUP BY st.id ORDER BY st.name"
                .to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let streamers = rows
        .iter()
        .map(|row| StreamerInfo {
            id: row.try_get("", "id").unwrap_or(0),
            platform: row.try_get("", "platform").unwrap_or_default(),
            room_id: row.try_get("", "room_id").ok(),
            name: row.try_get("", "name").unwrap_or_default(),
            video_count: row.try_get("", "video_count").unwrap_or(0),
        })
        .collect();

    Ok(streamers)
}

// ==================== Audio Envelope ====================

/// Extract audio volume envelope for a video and store in database
#[tauri::command]
pub async fn extract_envelope(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<Vec<f32>, String> {
    // Get video path
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT file_path FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let file_path: String = row.try_get("", "file_path").unwrap_or_default();
    let path = std::path::Path::new(&file_path);

    // Extract envelope (500ms windows)
    let envelope = ffmpeg::extract_audio_envelope(&state.ffmpeg_path, path, 500).await?;

    // Serialize values to binary blob
    let data_bytes: Vec<u8> = envelope
        .values
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();

    // Store in database
    let data_hex = hex::encode(&data_bytes);
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT OR REPLACE INTO audio_envelopes (video_id, window_ms, data) VALUES ({}, {}, X'{}')",
            video_id, envelope.window_ms, data_hex
        ),
    )
    .await
    .map_err(|e| format!("存储音量数据失败: {}", e))?;

    // Mark video as having envelope
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("UPDATE videos SET has_envelope = 1 WHERE id = {}", video_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!("Audio envelope extracted: video_id={}, {} points", video_id, envelope.values.len());

    Ok(envelope.values)
}

/// Get stored audio envelope for a video
#[tauri::command]
pub async fn get_envelope(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<Option<Vec<f32>>, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT data FROM audio_envelopes WHERE video_id = {}", video_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    match row {
        Some(row) => {
            let data: Vec<u8> = row.try_get("", "data").unwrap_or_default();
            let values: Vec<f32> = data
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            Ok(Some(values))
        }
        None => Ok(None),
    }
}

/// Helper: convert a query row to VideoInfo
fn row_to_video_info(row: &sea_orm::QueryResult) -> VideoInfo {
    VideoInfo {
        id: row.try_get("", "id").unwrap_or(0),
        file_path: row.try_get("", "file_path").unwrap_or_default(),
        file_name: row.try_get("", "file_name").unwrap_or_default(),
        file_hash: row.try_get("", "file_hash").ok(),
        file_size: row.try_get("", "file_size").unwrap_or(0),
        duration_ms: row.try_get("", "duration_ms").ok(),
        width: row.try_get("", "width").ok(),
        height: row.try_get("", "height").ok(),
        format_name: None, // Not stored in DB yet
        video_codec: None,
        audio_codec: None,
        has_subtitle: row.try_get::<bool>("", "has_subtitle").unwrap_or(false),
        has_danmaku: row.try_get::<bool>("", "has_danmaku").unwrap_or(false),
        has_envelope: row.try_get::<bool>("", "has_envelope").unwrap_or(false),
        workspace_id: row.try_get("", "workspace_id").ok(),
        session_id: row.try_get("", "session_id").ok(),
        streamer_id: row.try_get("", "streamer_id").ok(),
        streamer_name: row.try_get("", "streamer_name").ok(),
        stream_title: row.try_get("", "stream_title").ok(),
        recorded_at: row.try_get("", "recorded_at").ok(),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
    }
}
