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
