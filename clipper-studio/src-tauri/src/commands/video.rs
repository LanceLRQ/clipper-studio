use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use crate::utils::{ffmpeg, hash};

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
                "SELECT * FROM videos WHERE file_path = '{}'",
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
        Some(id) => format!("WHERE workspace_id = {}", id),
        None => String::new(),
    };

    // Count total
    let count_row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT COUNT(*) as cnt FROM videos {}", where_clause),
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
                "SELECT * FROM videos {} ORDER BY created_at DESC LIMIT {} OFFSET {}",
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
            format!("SELECT * FROM videos WHERE id = {}", video_id),
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
        stream_title: row.try_get("", "stream_title").ok(),
        recorded_at: row.try_get("", "recorded_at").ok(),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
    }
}
