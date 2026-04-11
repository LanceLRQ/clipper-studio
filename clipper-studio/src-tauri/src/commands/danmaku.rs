use std::path::Path;

use tauri::State;

use crate::core::danmaku::{
    self, compute_density, normalize_density, DanmakuAssOptions, DanmakuItem,
};
use crate::AppState;

/// Parse danmaku from a video's associated XML file
#[tauri::command]
pub async fn load_danmaku(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<Vec<DanmakuItem>, String> {
    // Get video file path to find associated XML
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
    let video_path = Path::new(&file_path);

    // Look for .xml file with same stem
    let xml_path = video_path.with_extension("xml");
    if !xml_path.exists() {
        return Err("未找到关联的弹幕 XML 文件".to_string());
    }

    danmaku::parse_bilibili_xml(&xml_path)
}

/// Get danmaku density data (for heatmap overlay)
#[tauri::command]
pub async fn get_danmaku_density(
    state: State<'_, AppState>,
    video_id: i64,
    window_ms: Option<i64>,
) -> Result<Vec<f32>, String> {
    let items = load_danmaku(state.clone(), video_id).await?;

    // Get video duration
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT duration_ms FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let duration_ms: i64 = row.try_get("", "duration_ms").unwrap_or(0);
    let win = window_ms.unwrap_or(5000);

    let density = compute_density(&items, duration_ms, win);
    Ok(normalize_density(&density))
}

/// Convert danmaku XML to ASS using DanmakuFactory
#[tauri::command]
pub async fn convert_danmaku_to_ass(
    state: State<'_, AppState>,
    video_id: i64,
    options: Option<DanmakuAssOptions>,
) -> Result<String, String> {
    let danmaku_factory_path = state.danmaku_factory_path.read().unwrap().clone();
    if danmaku_factory_path.is_empty() {
        return Err("DanmakuFactory 未安装。请在 config.toml 的 [tools] 中配置 danmaku_factory_path，或将 DanmakuFactory 放入系统 PATH".to_string());
    }

    // Get video file path
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT file_path, width, height FROM videos WHERE id = {}",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let file_path: String = row.try_get("", "file_path").unwrap_or_default();
    let video_path = Path::new(&file_path);

    let xml_path = video_path.with_extension("xml");
    if !xml_path.exists() {
        return Err("未找到关联的弹幕 XML 文件".to_string());
    }

    let ass_path = video_path.with_extension("danmaku.ass");

    // Use video resolution if available
    let mut opts = options.unwrap_or_default();
    if let Ok(w) = row.try_get::<i32>("", "width") {
        if let Ok(h) = row.try_get::<i32>("", "height") {
            if w > 0 && h > 0 {
                opts.width = w as u32;
                opts.height = h as u32;
            }
        }
    }

    danmaku::convert_to_ass(
        &danmaku_factory_path,
        &xml_path,
        &ass_path,
        &opts,
    )?;

    let output = ass_path.to_string_lossy().to_string();
    tracing::info!("Danmaku ASS generated: {}", output);
    Ok(output)
}

/// Check DanmakuFactory availability
#[tauri::command]
pub fn check_danmaku_factory(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(!state.danmaku_factory_path.read().unwrap().is_empty())
}
