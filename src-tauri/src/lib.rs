pub mod commands;
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

use tauri::Manager;

use crate::db::Database;
use crate::utils::ffmpeg;

/// Application shared state, injected via Tauri State
pub struct AppState {
    pub db: Database,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
}

/// Build and configure the Tauri application
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "clipper_studio_lib=debug,info".into()),
        )
        .init();

    tracing::info!("ClipperStudio starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Detect FFmpeg binaries
            let app_dir = app
                .path()
                .resource_dir()
                .unwrap_or_default();
            let bin_dir = app_dir.join("bin");

            let ffmpeg_path = ffmpeg::detect_binary("ffmpeg", &bin_dir);
            let ffprobe_path = ffmpeg::detect_binary("ffprobe", &bin_dir);

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

            // Initialize database
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("data.db");
            tracing::info!("Database path: {}", db_path.display());

            let db = tauri::async_runtime::block_on(async {
                Database::connect(&db_path).await
            })?;

            // Run migrations
            tauri::async_runtime::block_on(async {
                db.run_migrations().await
            })?;

            tracing::info!("Database initialized successfully");

            // Set up system tray
            shell::tray::setup_tray(app)?;

            // Manage application state
            app.manage(AppState {
                db,
                ffmpeg_path: ffmpeg_path.unwrap_or_default(),
                ffprobe_path: ffprobe_path.unwrap_or_default(),
            });

            tracing::info!("ClipperStudio ready!");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_app_info,
            commands::system::check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ClipperStudio");
}
