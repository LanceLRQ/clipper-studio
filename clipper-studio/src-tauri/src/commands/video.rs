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
    pub streamer_id: Option<i64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub search: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub tag_ids: Option<Vec<i64>>,
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
    let ffprobe_path = state.ffprobe_path.read().unwrap().clone();
    let probe_result = if !ffprobe_path.is_empty() {
        match ffmpeg::probe(&ffprobe_path, path) {
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

    // Build WHERE clauses dynamically
    let mut conditions: Vec<String> = Vec::new();

    if let Some(ws_id) = req.workspace_id {
        conditions.push(format!("v.workspace_id = {}", ws_id));
    }

    if let Some(sid) = req.streamer_id {
        if sid == -1 {
            conditions.push("v.streamer_id IS NULL".to_string());
        } else {
            conditions.push(format!("v.streamer_id = {}", sid));
        }
    }

    if let Some(ref keyword) = req.search {
        let escaped = keyword.replace('\'', "''").replace('%', "\\%");
        if !escaped.is_empty() {
            conditions.push(format!(
                "(v.stream_title LIKE '%{}%' OR v.file_name LIKE '%{}%')",
                escaped, escaped
            ));
        }
    }

    if let Some(ref d) = req.date_from {
        if !d.is_empty() {
            conditions.push(format!("v.recorded_at >= '{}'", d.replace('\'', "''")));
        }
    }

    if let Some(ref d) = req.date_to {
        if !d.is_empty() {
            // date_to is inclusive: use next day
            conditions.push(format!("v.recorded_at < '{}T'", d.replace('\'', "''")));
        }
    }

    // Tag filter: videos that have ALL specified tags
    let tag_join = if let Some(ref tag_ids) = req.tag_ids {
        let ids: Vec<String> = tag_ids.iter().filter(|id| **id > 0).map(|id| id.to_string()).collect();
        if !ids.is_empty() {
            let id_list = ids.join(",");
            let count = ids.len();
            conditions.push(format!(
                "v.id IN (SELECT video_id FROM video_tags WHERE tag_id IN ({}) GROUP BY video_id HAVING COUNT(DISTINCT tag_id) = {})",
                id_list, count
            ));
        }
        String::new()
    } else {
        String::new()
    };
    let _ = tag_join; // suppress unused warning

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Validate and build ORDER BY
    let sort_col = match req.sort_by.as_deref() {
        Some("recorded_at") => "v.recorded_at",
        _ => "v.created_at",
    };
    let sort_dir = match req.sort_order.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
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
                 {} ORDER BY {} {} LIMIT {} OFFSET {}",
                where_clause, sort_col, sort_dir, page_size, offset
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
    pub total_duration_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListStreamersRequest {
    pub workspace_id: Option<i64>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListStreamersResponse {
    pub streamers: Vec<StreamerInfo>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsRequest {
    pub workspace_id: Option<i64>,
    pub streamer_id: Option<i64>,
    pub sort_order: Option<String>,
    pub search: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

/// List videos grouped by recording sessions, with filtering and pagination
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
    req: Option<ListSessionsRequest>,
) -> Result<ListSessionsResponse, String> {
    let req = req.unwrap_or(ListSessionsRequest {
        workspace_id: None,
        streamer_id: None,
        sort_order: None,
        search: None,
        date_from: None,
        date_to: None,
        page: None,
        page_size: None,
    });

    let page = req.page.unwrap_or(1).max(1);
    let page_size = req.page_size.unwrap_or(50).min(200);
    let offset = (page - 1) * page_size;

    let sort_dir = match req.sort_order.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
    };

    // Build WHERE clauses
    let mut conditions: Vec<String> = Vec::new();
    if let Some(ws_id) = req.workspace_id {
        conditions.push(format!("s.workspace_id = {}", ws_id));
    }
    if let Some(sid) = req.streamer_id {
        if sid == -1 {
            conditions.push("s.streamer_id IS NULL".to_string());
        } else {
            conditions.push(format!("s.streamer_id = {}", sid));
        }
    }
    if let Some(ref keyword) = req.search {
        let escaped = keyword.replace('\'', "''").replace('%', "\\%");
        if !escaped.is_empty() {
            conditions.push(format!("s.title LIKE '%{}%'", escaped));
        }
    }
    if let Some(ref d) = req.date_from {
        if !d.is_empty() {
            conditions.push(format!("s.started_at >= '{}'", d.replace('\'', "''")));
        }
    }
    if let Some(ref d) = req.date_to {
        if !d.is_empty() {
            conditions.push(format!("s.started_at < '{}T'", d.replace('\'', "''")));
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count total
    let count_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT COUNT(*) as cnt FROM recording_sessions s {}", where_clause),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let total: i64 = count_row
        .and_then(|r| r.try_get("", "cnt").ok())
        .unwrap_or(0);

    // Query sessions page
    let session_rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT s.*, st.name as streamer_name FROM recording_sessions s \
                 LEFT JOIN streamers st ON s.streamer_id = st.id \
                 {} ORDER BY s.started_at {} LIMIT {} OFFSET {}",
                where_clause, sort_dir, page_size, offset
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    // 收集本页所有 session_id，后续用一次 IN 查询批量拉取所属视频，避免 N+1 查询
    let session_ids: Vec<i64> = session_rows
        .iter()
        .map(|r| r.try_get::<i64>("", "id").unwrap_or(0))
        .filter(|id| *id != 0)
        .collect();

    // 批量查询所有 session 的视频：一次 IN 查询替代 N 次单 session 查询
    let mut videos_by_session: std::collections::HashMap<i64, Vec<VideoInfo>> =
        std::collections::HashMap::new();

    if !session_ids.is_empty() {
        let ids_list = session_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let all_video_rows = sea_orm::ConnectionTrait::query_all(
            state.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT v.*, st.name as streamer_name FROM videos v \
                     LEFT JOIN streamers st ON v.streamer_id = st.id \
                     WHERE v.session_id IN ({}) ORDER BY v.session_id, v.recorded_at ASC",
                    ids_list
                ),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        for vrow in &all_video_rows {
            let sid: i64 = vrow.try_get("", "session_id").unwrap_or(0);
            if sid == 0 {
                continue;
            }
            videos_by_session
                .entry(sid)
                .or_default()
                .push(row_to_video_info(vrow));
        }
    }

    let mut sessions = Vec::with_capacity(session_rows.len());
    for srow in &session_rows {
        let session_id: i64 = srow.try_get("", "id").unwrap_or(0);
        let videos = videos_by_session.remove(&session_id).unwrap_or_default();

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

    Ok(ListSessionsResponse {
        sessions,
        total,
        page,
        page_size,
    })
}

/// List streamers with optional workspace filter and pagination
#[tauri::command]
pub async fn list_streamers(
    state: State<'_, AppState>,
    req: Option<ListStreamersRequest>,
) -> Result<ListStreamersResponse, String> {
    let req = req.unwrap_or(ListStreamersRequest {
        workspace_id: None,
        page: None,
        page_size: None,
    });
    let page = req.page.unwrap_or(1).max(1);
    let page_size = req.page_size.unwrap_or(50).min(200);
    let offset = (page - 1) * page_size;

    let ws_filter = match req.workspace_id {
        Some(id) => format!("WHERE v.workspace_id = {}", id),
        None => String::new(),
    };

    // Count total streamers (that have videos in this workspace)
    let count_sql = if ws_filter.is_empty() {
        "SELECT COUNT(*) as cnt FROM streamers".to_string()
    } else {
        format!(
            "SELECT COUNT(DISTINCT st.id) as cnt FROM streamers st \
             INNER JOIN videos v ON st.id = v.streamer_id {}",
            ws_filter
        )
    };

    let count_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, count_sql),
    )
    .await
    .map_err(|e| e.to_string())?;

    let total: i64 = count_row
        .and_then(|r| r.try_get("", "cnt").ok())
        .unwrap_or(0);

    // Query streamers with aggregated video stats
    let query_sql = if ws_filter.is_empty() {
        format!(
            "SELECT st.*, COUNT(v.id) as video_count, SUM(v.duration_ms) as total_duration_ms \
             FROM streamers st LEFT JOIN videos v ON st.id = v.streamer_id \
             GROUP BY st.id ORDER BY st.name LIMIT {} OFFSET {}",
            page_size, offset
        )
    } else {
        format!(
            "SELECT st.*, COUNT(v.id) as video_count, SUM(v.duration_ms) as total_duration_ms \
             FROM streamers st INNER JOIN videos v ON st.id = v.streamer_id {} \
             GROUP BY st.id ORDER BY st.name LIMIT {} OFFSET {}",
            ws_filter, page_size, offset
        )
    };

    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, query_sql),
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
            total_duration_ms: row.try_get("", "total_duration_ms").ok(),
        })
        .collect();

    Ok(ListStreamersResponse {
        streamers,
        total,
        page,
        page_size,
    })
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
    let ffmpeg_path = state.ffmpeg_path.read().unwrap().clone();
    let envelope = ffmpeg::extract_audio_envelope(&ffmpeg_path, path, 500).await?;

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

// ==================== FLV Repair ====================

/// Check video file integrity
#[tauri::command]
pub async fn check_video_integrity(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<ffmpeg::IntegrityResult, String> {
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
    let ffprobe_path = state.ffprobe_path.read().unwrap().clone();
    ffmpeg::check_integrity(&ffprobe_path, std::path::Path::new(&file_path))
}

/// Remux a video file to MP4 (stream copy, fixes most FLV issues)
#[tauri::command]
pub async fn remux_video(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<String, String> {
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
    let input = std::path::Path::new(&file_path);

    // Output: same directory, same name but .mp4 extension
    let output = input.with_extension("remux.mp4");
    if output.exists() {
        return Err("修复后的文件已存在".to_string());
    }

    let ffmpeg_path = state.ffmpeg_path.read().unwrap().clone();
    ffmpeg::remux_to_mp4(&ffmpeg_path, input, &output).await?;

    let output_str = output.to_string_lossy().to_string();
    tracing::info!("Remuxed video {} -> {}", file_path, output_str);
    Ok(output_str)
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
