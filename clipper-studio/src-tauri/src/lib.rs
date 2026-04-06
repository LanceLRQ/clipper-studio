pub mod commands;
pub mod config;
pub mod core;
pub mod db;
pub mod shell;
pub mod utils;

// Phase 2+
// pub mod asr;
// pub mod plugin;

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
}

/// Build and configure the Tauri application
pub fn run() {
    // Pre-init: load config to get log level before full setup
    // We do a minimal init here; full config is loaded in setup()
    let pre_log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("clipper_studio_lib={},info", pre_log_level).into()),
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

            // Set up system tray
            shell::tray::setup_tray(app)?;

            // Manage application state
            app.manage(AppState {
                db,
                config: RwLock::new(app_config),
                config_dir: data_dir,
                ffmpeg_path: ffmpeg_path.unwrap_or_default(),
                ffprobe_path: ffprobe_path.unwrap_or_default(),
                media_server_port,
                task_queue,
            });

            tracing::info!("ClipperStudio ready!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_app_info,
            commands::system::check_ffmpeg,
            commands::video::import_video,
            commands::video::list_videos,
            commands::video::get_video,
            commands::video::delete_video,
            commands::video::list_sessions,
            commands::video::list_streamers,
            commands::video::extract_envelope,
            commands::video::get_envelope,
            commands::clip::create_clip,
            commands::clip::cancel_clip,
            commands::clip::list_clip_tasks,
            commands::clip::list_presets,
            commands::workspace::list_workspaces,
            commands::workspace::create_workspace,
            commands::workspace::delete_workspace,
            commands::workspace::get_active_workspace,
            commands::workspace::set_active_workspace,
            commands::workspace::scan_workspace,
            commands::workspace::detect_workspace_adapter,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ClipperStudio");
}
