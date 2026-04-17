pub mod commands;
pub mod config;
pub mod core;
pub mod db;
pub mod deps;
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
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use std::sync::Arc;

use crate::asr::manager::ASRServiceManager;
use crate::asr::queue::ASRTaskQueue;
use crate::config::AppConfig;
use crate::core::media_server::MediaServer;
use crate::core::queue::TaskQueue;
use crate::core::watcher::WorkspaceWatcher;
use crate::deps::DependencyManager;
use crate::plugin::manager::PluginManager;
use crate::plugin::registry::PluginRegistry;
use crate::db::Database;
use crate::utils::ffmpeg;

/// Application shared state, injected via Tauri State
pub struct AppState {
    pub db: Database,
    pub config: RwLock<AppConfig>,
    pub config_dir: std::path::PathBuf,
    pub ffmpeg_path: Arc<RwLock<String>>,
    pub ffprobe_path: RwLock<String>,
    pub media_server: Arc<MediaServer>,
    pub media_server_port: u16,
    pub task_queue: Arc<TaskQueue>,
    pub danmaku_factory_path: RwLock<String>,
    pub watcher: Arc<WorkspaceWatcher>,
    pub plugin_manager: Arc<PluginManager>,
    pub plugin_registry: Arc<PluginRegistry>,
    pub asr_service_manager: Arc<ASRServiceManager>,
    pub asr_task_queue: Arc<ASRTaskQueue>,
    pub dep_manager: Arc<DependencyManager>,
    pub bin_dir: std::path::PathBuf,
    pub debug_mode: bool,
}

/// Build and configure the Tauri application
pub fn run() {
    // 解析命令行参数：是否启用调试模式（显示 webview devtools 与路由调试面板）
    // 仅在 debug 构建下生效，生产版本强制禁用以防滥用
    let debug_mode = cfg!(debug_assertions) && std::env::args().any(|a| a == "--debug");

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
        .setup(move |app| {
            // Resolve data directory
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;

            // Load config from config.toml
            let app_config = AppConfig::load(&data_dir);
            tracing::info!("Log level from config: {}", app_config.general.log_level);

            // Initialize dependency manager
            let deps_dir = data_dir.join("deps");
            let proxy_url = if app_config.network.proxy_url.is_empty() {
                None
            } else {
                Some(app_config.network.proxy_url.as_str())
            };
            let dep_manager = Arc::new(DependencyManager::new(deps_dir.clone(), proxy_url));
            dep_manager.refresh_all();

            // Resolve FFmpeg paths: config override > deps dir > bin dir > system PATH
            let resource_dir = app.path().resource_dir().unwrap_or_default();
            let bin_dir = resource_dir.join("bin");

            let ffmpeg_path = if !app_config.ffmpeg.ffmpeg_path.is_empty() {
                tracing::info!("FFmpeg path from config: {}", app_config.ffmpeg.ffmpeg_path);
                Some(app_config.ffmpeg.ffmpeg_path.clone())
            } else if let Some(p) = dep_manager.get_binary_path("ffmpeg", "ffmpeg") {
                tracing::info!("FFmpeg found via deps manager: {}", p.display());
                Some(p.to_string_lossy().to_string())
            } else {
                ffmpeg::detect_binary("ffmpeg", &bin_dir)
            };

            let ffprobe_path = if !app_config.ffmpeg.ffprobe_path.is_empty() {
                tracing::info!("FFprobe path from config: {}", app_config.ffmpeg.ffprobe_path);
                Some(app_config.ffmpeg.ffprobe_path.clone())
            } else if let Some(p) = dep_manager.get_binary_path("ffmpeg", "ffprobe") {
                tracing::info!("FFprobe found via deps manager: {}", p.display());
                Some(p.to_string_lossy().to_string())
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

            // Detect danmaku tool: config > deps (DanmakuFactory or dmconvert) > bin dir > PATH
            let danmaku_factory_path = if !app_config.tools.danmaku_factory_path.is_empty() {
                tracing::info!("Danmaku tool path from config: {}", app_config.tools.danmaku_factory_path);
                Some(app_config.tools.danmaku_factory_path.clone())
            } else if let Some(p) = dep_manager.get_binary_path("danmaku-factory", "DanmakuFactory") {
                tracing::info!("Danmaku tool found via deps manager: {}", p.display());
                Some(p.to_string_lossy().to_string())
            } else {
                ffmpeg::detect_binary("DanmakuFactory", &bin_dir)
                    .or_else(|| ffmpeg::detect_binary("dmconvert", &bin_dir))
            };

            if let Some(ref path) = danmaku_factory_path {
                tracing::info!("Danmaku tool found: {}", path);
            } else {
                tracing::info!("Danmaku tool not found. Danmaku ASS conversion will be unavailable.");
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

            // Recover tasks that were left in 'processing' state due to abnormal exit
            tauri::async_runtime::block_on(async {
                let tables = ["clip_tasks", "asr_tasks", "media_tasks"];
                for table in &tables {
                    let sql = format!(
                        "UPDATE {} SET status = 'failed', error_message = '应用异常退出，任务已重置' \
                         WHERE status = 'processing'",
                        table
                    );
                    match sea_orm::ConnectionTrait::execute_unprepared(db.conn(), &sql).await {
                        Ok(result) => {
                            let count = result.rows_affected();
                            if count > 0 {
                                tracing::warn!(
                                    "Recovered {} stuck '{}' tasks in table '{}'",
                                    count, "processing", table
                                );
                            }
                        }
                        Err(e) => {
                            // Table might not exist yet in early versions, ignore
                            tracing::debug!("Skip recovery for '{}': {}", table, e);
                        }
                    }
                }
            });

            // Start local media server
            let media_server = tauri::async_runtime::block_on(async {
                MediaServer::start().await
            })?;
            let media_server_port = media_server.port();
            let media_server = Arc::new(media_server);

            // 默认允许应用数据目录下的 remux 缓存等文件（若后续使用）
            media_server.allow_prefix(&data_dir);

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
                use clipper_studio_plugin_storage::StorageProviderPluginBuilder;
                tracing::info!("Registering builtin plugins...");
                plugin_registry.register(BilibiliRecorderPluginBuilder::new());
                plugin_registry.register(StorageProviderPluginBuilder::new());
                tracing::info!("Builtin plugins registered (BilibiliRecorder, StorageProvider)");
            }
            let plugin_registry = Arc::new(plugin_registry);

            // Initialize workspace file watcher
            let watcher = Arc::new(WorkspaceWatcher::new(app.handle().clone()));

            // Initialize ASR service manager
            let asr_service_manager = Arc::new(ASRServiceManager::new());

            // Create shared ffmpeg_path for ASR task queue
            let ffmpeg_path_shared = Arc::new(RwLock::new(ffmpeg_path.unwrap_or_default()));

            // Initialize ASR task queue (must be after db init)
            let asr_task_queue = Arc::new(ASRTaskQueue::new(
                app.handle().clone(),
                db.clone(),
                ffmpeg_path_shared.clone(),
            ));

            // Set up system tray
            shell::tray::setup_tray(app)?;

            // Manage application state
            app.manage(AppState {
                db: db.clone(),
                config: RwLock::new(app_config),
                config_dir: data_dir,
                ffmpeg_path: ffmpeg_path_shared,
                ffprobe_path: RwLock::new(ffprobe_path.unwrap_or_default()),
                danmaku_factory_path: RwLock::new(danmaku_factory_path.unwrap_or_default()),
                media_server: media_server.clone(),
                media_server_port,
                task_queue,
                watcher: watcher.clone(),
                plugin_manager: plugin_manager.clone(),
                plugin_registry: plugin_registry.clone(),
                asr_service_manager: asr_service_manager.clone(),
                asr_task_queue: asr_task_queue.clone(),
                dep_manager: dep_manager.clone(),
                bin_dir: bin_dir.clone(),
                debug_mode,
            });

            // 根据 --debug 参数控制 webview devtools 的开关
            if let Some(main_window) = app.get_webview_window("main") {
                if debug_mode {
                    tracing::info!("Debug mode enabled: opening webview devtools");
                    main_window.open_devtools();
                } else {
                    main_window.close_devtools();
                }
            }

            // Start ASR task queue worker (after AppState is managed)
            asr_task_queue.start();

            // Start watching existing workspaces with auto_scan enabled，
            // 同时为所有 workspace 路径及其 clip_output_dir 登记 media_server 白名单
            let watcher_clone = watcher.clone();
            let media_server_clone = media_server.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(rows) = sea_orm::ConnectionTrait::query_all(
                    db.conn(),
                    sea_orm::Statement::from_string(
                        sea_orm::DatabaseBackend::Sqlite,
                        "SELECT id, path, clip_output_dir, auto_scan FROM workspaces".to_string(),
                    ),
                )
                .await
                {
                    for row in &rows {
                        let id: i64 = row.try_get("", "id").unwrap_or(0);
                        let path: String = row.try_get("", "path").unwrap_or_default();
                        let clip_dir: Option<String> = row
                            .try_get::<Option<String>>("", "clip_output_dir")
                            .unwrap_or(None);
                        let auto_scan: bool = row.try_get("", "auto_scan").unwrap_or(true);

                        if !path.is_empty() {
                            media_server_clone.allow_prefix(&path);
                        }
                        if let Some(dir) = clip_dir.as_ref().filter(|s| !s.is_empty()) {
                            media_server_clone.allow_prefix(dir);
                        }

                        if auto_scan && !path.is_empty() {
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
            commands::system::is_debug_mode,
            commands::system::track_event,
            commands::system::get_setting,
            commands::system::set_setting,
            commands::system::get_settings,
            commands::system::reveal_file,
            commands::system::open_file,
            commands::system::confirm_close,
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
            commands::clip::retry_clip_task,
            commands::clip::list_clip_tasks,
            commands::clip::list_presets,
            commands::clip::create_batch_clips,
            commands::clip::check_video_burn_availability,
            commands::clip::auto_segment,
            commands::clip::has_active_clip_tasks,
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
            commands::workspace::cancel_scan,
            commands::workspace::detect_workspace_adapter,
            commands::workspace::check_workspace_path,
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
            commands::asr::validate_asr_path,
            commands::asr::start_asr_service,
            commands::asr::stop_asr_service,
            commands::asr::open_asr_setup_terminal,
            commands::asr::get_asr_service_status,
            commands::asr::get_asr_service_logs,
            commands::asr::submit_asr_queued,
            commands::asr::cancel_asr_task,
            commands::asr::get_asr_queue_snapshot,
            commands::asr::repair_subtitle_timestamps,
            commands::asr::check_docker_capability,
            commands::asr::check_docker_image_pulled,
            commands::asr::open_docker_pull_terminal,
            commands::asr::force_remove_asr_container,
            commands::asr::get_default_asr_docker_data_dir,
            commands::tag::create_tag,
            commands::tag::list_tags,
            commands::tag::update_tag,
            commands::tag::delete_tag,
            commands::tag::get_video_tags,
            commands::tag::set_video_tags,
            commands::deps::list_deps,
            commands::deps::check_dep,
            commands::deps::install_dep,
            commands::deps::uninstall_dep,
            commands::deps::set_dep_custom_path,
            commands::deps::reveal_dep_dir,
            commands::deps::set_deps_proxy,
            commands::deps::get_deps_proxy,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let has_clip = state.task_queue.has_active_tasks_sync();
                let has_asr = state.asr_task_queue.has_active_tasks();

                if has_clip || has_asr {
                    api.prevent_close();

                    let mut parts = Vec::new();
                    if has_clip { parts.push("切片任务"); }
                    if has_asr { parts.push("ASR 识别任务"); }
                    let message = format!(
                        "当前有{}正在运行，关闭应用将中断这些任务。确定要关闭吗？",
                        parts.join("和")
                    );

                    let win = window.clone();
                    window.dialog()
                        .message(message)
                        .title("确认关闭")
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCancelCustom("确认关闭".to_string(), "取消".to_string()))
                        .show(move |confirmed| {
                            if confirmed {
                                let _ = win.destroy();
                            }
                        });
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building ClipperStudio")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                tracing::info!("ClipperStudio exiting, cleaning up...");
                let state = app_handle.state::<AppState>();

                // Cancel all active clip/merge/media tasks to terminate FFmpeg processes
                let task_queue = state.task_queue.clone();
                tauri::async_runtime::block_on(async {
                    task_queue.cancel_all().await;
                });

                // Stop managed ASR service if running
                let asr_mgr = state.asr_service_manager.clone();
                if asr_mgr.is_running() {
                    tracing::info!("Stopping managed ASR service...");
                    tauri::async_runtime::block_on(async {
                        let _ = asr_mgr.stop().await;
                    });
                    tracing::info!("ASR service stopped");
                }

                let plugin_manager = state.plugin_manager.clone();
                let plugin_registry = state.plugin_registry.clone();
                let db = state.db.clone();
                tauri::async_runtime::block_on(async {
                    // Shutdown managed plugin services (child processes)
                    plugin_manager.shutdown_all().await;

                    // Check if auto-unmount is enabled before shutting down builtin plugins
                    let auto_unmount = match sea_orm::ConnectionTrait::query_one(
                        db.conn(),
                        sea_orm::Statement::from_string(
                            sea_orm::DatabaseBackend::Sqlite,
                            "SELECT value FROM settings_kv WHERE key = 'plugin:builtin.storage.smb:auto_unmount_on_exit'".to_string(),
                        ),
                    ).await {
                        Ok(Some(row)) => row.try_get::<String>("", "value").unwrap_or_default() == "true",
                        _ => false,
                    };

                    if auto_unmount {
                        tracing::info!("Auto-unmount enabled, shutting down builtin plugins...");
                        plugin_registry.shutdown_all().await;
                    } else {
                        tracing::info!("Auto-unmount disabled, skipping SMB unmount");
                    }
                });
                tracing::info!("Plugin cleanup complete");
            }
        });
}
