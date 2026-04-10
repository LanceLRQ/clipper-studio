pub mod commands;
pub mod config;
pub mod core;
pub mod db;
pub mod shell;
pub mod utils;

pub mod plugin;
pub mod asr;

// Phase 5+
// pub mod llm;
// pub mod agent;
// pub mod sync;

use std::sync::RwLock;

use tauri::Manager;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::core::media_server::MediaServer;
use crate::core::queue::TaskQueue;
use crate::core::watcher::WorkspaceWatcher;
use crate::plugin::manager::PluginManager;
use crate::plugin::registry::PluginRegistry;
use crate::db::Database;
use crate::utils::ffmpeg;

/// Application shared state, injected via Tauri State
pub struct AppState {
    pub db: Database,
    pub config: RwLock<AppConfig>,
    pub config_dir: std::path::PathBuf,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub media_server_port: u16,
    pub task_queue: Arc<TaskQueue>,
    pub danmaku_factory_path: String,
    pub watcher: Arc<WorkspaceWatcher>,
    pub plugin_manager: Arc<PluginManager>,
    pub plugin_registry: Arc<PluginRegistry>,
}

/// Build and configure the Tauri application
pub fn run() {
    // Pre-init: load config to get log level before full setup
    // We do a minimal init here; full config is loaded in setup()
    let pre_log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    // dev 模式下默认开启自身 crate 的 debug 日志，其他 crate 保持 info
    let default_filter = if cfg!(debug_assertions) {
        format!(
            "clipper_studio_lib={lvl},clipper_studio_plugin_recorder={lvl},info",
            lvl = pre_log_level
        )
    } else {
        format!("clipper_studio_lib={},info", pre_log_level)
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .init();

    tracing::info!("ClipperStudio starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Resolve data directory
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;

            // Load config from config.toml
            let app_config = AppConfig::load(&data_dir);
            tracing::info!("Log level from config: {}", app_config.general.log_level);

            // Resolve FFmpeg paths: config override > bin dir > system PATH
            let resource_dir = app.path().resource_dir().unwrap_or_default();
            let bin_dir = resource_dir.join("bin");

            let ffmpeg_path = if !app_config.ffmpeg.ffmpeg_path.is_empty() {
                tracing::info!("FFmpeg path from config: {}", app_config.ffmpeg.ffmpeg_path);
                Some(app_config.ffmpeg.ffmpeg_path.clone())
            } else {
                ffmpeg::detect_binary("ffmpeg", &bin_dir)
            };

            let ffprobe_path = if !app_config.ffmpeg.ffprobe_path.is_empty() {
                tracing::info!("FFprobe path from config: {}", app_config.ffmpeg.ffprobe_path);
                Some(app_config.ffmpeg.ffprobe_path.clone())
            } else {
                ffmpeg::detect_binary("ffprobe", &bin_dir)
            };

            if let Some(ref path) = ffmpeg_path {
                tracing::info!("FFmpeg found: {}", path);
            } else {
                tracing::warn!("FFmpeg not found! Media features will be unavailable.");
            }
            if let Some(ref path) = ffprobe_path {
                tracing::info!("FFprobe found: {}", path);
            } else {
                tracing::warn!("FFprobe not found! Media probe features will be unavailable.");
            }

            // Detect DanmakuFactory binary
            let danmaku_factory_path = if !app_config.tools.danmaku_factory_path.is_empty() {
                tracing::info!("DanmakuFactory path from config: {}", app_config.tools.danmaku_factory_path);
                Some(app_config.tools.danmaku_factory_path.clone())
            } else {
                ffmpeg::detect_binary("DanmakuFactory", &bin_dir)
            };

            if let Some(ref path) = danmaku_factory_path {
                tracing::info!("DanmakuFactory found: {}", path);
            } else {
                tracing::info!("DanmakuFactory not found. Danmaku ASS conversion will be unavailable.");
            }

            // Initialize database (path from config)
            let db_path = app_config.resolve_db_path(&data_dir);
            tracing::info!("Database path: {}", db_path.display());

            let db = tauri::async_runtime::block_on(async {
                Database::connect(&db_path).await
            })?;

            tauri::async_runtime::block_on(async {
                db.run_migrations().await
            })?;

            tracing::info!("Database initialized successfully");

            // Start local media server
            let media_server = tauri::async_runtime::block_on(async {
                MediaServer::start().await
            })?;
            let media_server_port = media_server.port();

            // Initialize task queue (max 2 concurrent tasks)
            let task_queue = Arc::new(TaskQueue::new(app.handle().clone(), 2));

            // Initialize plugin manager
            let plugin_dir = data_dir.join("plugins");
            let _ = std::fs::create_dir_all(&plugin_dir);
            let plugin_manager = Arc::new(PluginManager::new());

            // Initialize plugin registry (for builtin plugins)
            let mut plugin_registry = PluginRegistry::new();
            // Register builtin plugins
            #[cfg(feature = "builtin-plugins")]
            {
                use clipper_studio_plugin_recorder::BilibiliRecorderPluginBuilder;
                tracing::info!("Registering builtin plugins...");
                plugin_registry.register(BilibiliRecorderPluginBuilder::new());
                tracing::info!("Builtin BilibiliRecorder plugin registered");
            }
            let plugin_registry = Arc::new(plugin_registry);

            // Initialize workspace file watcher
            let watcher = Arc::new(WorkspaceWatcher::new(app.handle().clone()));

            // Set up system tray
            shell::tray::setup_tray(app)?;

            // Manage application state
            app.manage(AppState {
                db: db.clone(),
                config: RwLock::new(app_config),
                config_dir: data_dir,
                ffmpeg_path: ffmpeg_path.unwrap_or_default(),
                ffprobe_path: ffprobe_path.unwrap_or_default(),
                danmaku_factory_path: danmaku_factory_path.unwrap_or_default(),
                media_server_port,
                task_queue,
                watcher: watcher.clone(),
                plugin_manager: plugin_manager.clone(),
                plugin_registry: plugin_registry.clone(),
            });

            // Start watching existing workspaces with auto_scan enabled
            let watcher_clone = watcher.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(rows) = sea_orm::ConnectionTrait::query_all(
                    db.conn(),
                    sea_orm::Statement::from_string(
                        sea_orm::DatabaseBackend::Sqlite,
                        "SELECT id, path FROM workspaces WHERE auto_scan = 1".to_string(),
                    ),
                )
                .await
                {
                    for row in &rows {
                        let id: i64 = row.try_get("", "id").unwrap_or(0);
                        let path: String = row.try_get("", "path").unwrap_or_default();
                        if !path.is_empty() {
                            if let Err(e) = watcher_clone.watch(id, std::path::Path::new(&path)) {
                                tracing::warn!("Failed to watch workspace {}: {}", id, e);
                            }
                        }
                    }
                }
            });

            tracing::info!("ClipperStudio ready!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_app_info,
            commands::system::get_dashboard_stats,
            commands::system::check_ffmpeg,
            commands::system::track_event,
            commands::system::get_setting,
            commands::system::set_setting,
            commands::system::get_settings,
            commands::system::reveal_file,
            commands::system::open_file,
            commands::video::import_video,
            commands::video::list_videos,
            commands::video::get_video,
            commands::video::delete_video,
            commands::video::list_sessions,
            commands::video::list_streamers,
            commands::video::extract_envelope,
            commands::video::get_envelope,
            commands::video::check_video_integrity,
            commands::video::remux_video,
            commands::clip::create_clip,
            commands::clip::cancel_clip,
            commands::clip::list_clip_tasks,
            commands::clip::list_presets,
            commands::clip::create_batch_clips,
            commands::clip::check_video_burn_availability,
            commands::clip::auto_segment,
            commands::clip::delete_clip_task,
            commands::clip::delete_clip_batch,
            commands::clip::clear_finished_clip_tasks,
            commands::media::transcode_video,
            commands::media::merge_videos,
            commands::media::list_media_tasks,
            commands::media::delete_media_task,
            commands::media::clear_finished_media_tasks,
            commands::workspace::list_workspaces,
            commands::workspace::create_workspace,
            commands::workspace::update_workspace,
            commands::workspace::delete_workspace,
            commands::workspace::get_active_workspace,
            commands::workspace::set_active_workspace,
            commands::workspace::scan_workspace,
            commands::workspace::detect_workspace_adapter,
            commands::workspace::get_disk_usage,
            commands::plugin::scan_plugins,
            commands::plugin::list_plugins,
            commands::plugin::load_plugin,
            commands::plugin::unload_plugin,
            commands::plugin::start_plugin_service,
            commands::plugin::stop_plugin_service,
            commands::plugin::call_plugin,
            commands::plugin::get_plugin_config,
            commands::plugin::set_plugin_config,
            commands::plugin::set_plugin_enabled,
            commands::plugin::auto_load_plugins,
            commands::asr::submit_asr,
            commands::asr::poll_asr,
            commands::asr::list_asr_tasks,
            commands::asr::list_subtitles,
            commands::asr::search_subtitles,
            commands::asr::search_subtitles_global,
            commands::asr::check_asr_health,
            commands::danmaku::load_danmaku,
            commands::danmaku::get_danmaku_density,
            commands::danmaku::convert_danmaku_to_ass,
            commands::danmaku::check_danmaku_factory,
            commands::asr::update_subtitle,
            commands::asr::delete_subtitle,
            commands::asr::merge_subtitles,
            commands::asr::split_subtitle,
            commands::asr::export_subtitles_srt,
            commands::asr::export_subtitles_ass,
            commands::asr::export_subtitles_vtt,
            commands::tag::create_tag,
            commands::tag::list_tags,
            commands::tag::update_tag,
            commands::tag::delete_tag,
            commands::tag::get_video_tags,
            commands::tag::set_video_tags,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ClipperStudio");
}
