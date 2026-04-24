use crate::utils::locks::RwLockExt;
use tauri::{AppHandle, State};

use crate::deps::config_overrides_from_app_config;
use crate::deps::registry::{DepStatus, DependencyStatus};
use crate::AppState;

/// List all managed dependencies and their status
#[tauri::command]
pub async fn list_deps(state: State<'_, AppState>) -> Result<Vec<DependencyStatus>, String> {
    let config = state.config.read().map_err(|e| e.to_string())?;
    let overrides = config_overrides_from_app_config(&config);
    Ok(state.dep_manager.list_deps(&overrides, &state.bin_dir))
}

/// Check a single dependency status (force re-detect)
#[tauri::command]
pub async fn check_dep(
    dep_id: String,
    state: State<'_, AppState>,
) -> Result<DependencyStatus, String> {
    let config = state.config.read().map_err(|e| e.to_string())?;
    let overrides = config_overrides_from_app_config(&config);
    let status = state
        .dep_manager
        .check_dep(&dep_id, &overrides, &state.bin_dir)?;
    drop(config);
    if status.status == DepStatus::Installed || status.system_available {
        refresh_tool_paths(&dep_id, &state);
    }
    Ok(status)
}

/// Install a dependency (download + extract + verify)
#[tauri::command]
pub async fn install_dep(
    dep_id: String,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proxy_url = {
        let config = state.config.read().map_err(|e| e.to_string())?;
        let url = config.network.proxy_url.clone();
        if url.is_empty() {
            None
        } else {
            Some(url)
        }
    };
    state
        .dep_manager
        .install_dep(&dep_id, &app_handle, proxy_url.as_deref())
        .await?;

    // Hot-refresh: update AppState paths so the new binary is usable immediately
    refresh_tool_paths(&dep_id, &state);

    Ok(())
}

/// Refresh tool paths in AppState after install/uninstall
fn refresh_tool_paths(dep_id: &str, state: &AppState) {
    match dep_id {
        "ffmpeg" => {
            if let Some(p) = state.dep_manager.get_binary_path("ffmpeg", "ffmpeg") {
                if let Ok(mut path) = state.ffmpeg_path.write() {
                    *path = p.to_string_lossy().to_string();
                    tracing::info!("Hot-refreshed ffmpeg_path: {}", *path);
                }
            }
            if let Some(p) = state.dep_manager.get_binary_path("ffmpeg", "ffprobe") {
                if let Ok(mut path) = state.ffprobe_path.write() {
                    *path = p.to_string_lossy().to_string();
                    tracing::info!("Hot-refreshed ffprobe_path: {}", *path);
                }
            }
        }
        "danmaku-factory" => {
            // get_binary_path handles both DanmakuFactory (Windows) and dmconvert (macOS venv)
            if let Some(p) = state
                .dep_manager
                .get_binary_path("danmaku-factory", "DanmakuFactory")
            {
                if let Ok(mut path) = state.danmaku_factory_path.write() {
                    *path = p.to_string_lossy().to_string();
                    tracing::info!("Hot-refreshed danmaku tool path: {}", *path);
                }
            }
        }
        _ => {}
    }
}

/// Cancel an in-progress dependency installation
#[tauri::command]
pub async fn cancel_dep(dep_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.dep_manager.cancel_dep(&dep_id)
}

/// Uninstall a dependency
#[tauri::command]
pub async fn uninstall_dep(dep_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.dep_manager.uninstall_dep(&dep_id)?;

    // Re-detect paths from remaining sources (config > bin_dir > PATH)
    re_detect_tool_paths(&dep_id, &state);

    Ok(())
}

/// Re-detect tool paths after uninstall: config.toml > bin_dir > system PATH
fn re_detect_tool_paths(dep_id: &str, state: &AppState) {
    use crate::utils::ffmpeg;

    let config = state.config.read_safe();

    match dep_id {
        "ffmpeg" => {
            // Re-detect ffmpeg
            let new_ffmpeg = if !config.ffmpeg.ffmpeg_path.is_empty() {
                Some(config.ffmpeg.ffmpeg_path.clone())
            } else {
                ffmpeg::detect_binary("ffmpeg", &state.bin_dir)
            };
            if let Ok(mut path) = state.ffmpeg_path.write() {
                let val = new_ffmpeg.unwrap_or_default();
                tracing::info!(
                    "Re-detected ffmpeg_path: {}",
                    if val.is_empty() { "(empty)" } else { &val }
                );
                *path = val;
            }

            // Re-detect ffprobe
            let new_ffprobe = if !config.ffmpeg.ffprobe_path.is_empty() {
                Some(config.ffmpeg.ffprobe_path.clone())
            } else {
                ffmpeg::detect_binary("ffprobe", &state.bin_dir)
            };
            if let Ok(mut path) = state.ffprobe_path.write() {
                let val = new_ffprobe.unwrap_or_default();
                tracing::info!(
                    "Re-detected ffprobe_path: {}",
                    if val.is_empty() { "(empty)" } else { &val }
                );
                *path = val;
            }
        }
        "danmaku-factory" => {
            let new_path = if !config.tools.danmaku_factory_path.is_empty() {
                Some(config.tools.danmaku_factory_path.clone())
            } else {
                ffmpeg::detect_binary("DanmakuFactory", &state.bin_dir)
                    .or_else(|| ffmpeg::detect_binary("dmconvert", &state.bin_dir))
            };
            if let Ok(mut path) = state.danmaku_factory_path.write() {
                let val = new_path.unwrap_or_default();
                tracing::info!(
                    "Re-detected danmaku tool path: {}",
                    if val.is_empty() { "(empty)" } else { &val }
                );
                *path = val;
            }
        }
        _ => {}
    }
}

/// Set a custom path for a dependency (writes to config.toml)
#[tauri::command]
pub async fn set_dep_custom_path(
    dep_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut config = state.config.write().map_err(|e| e.to_string())?;

    match dep_id.as_str() {
        "ffmpeg" => {
            // For ffmpeg, the path should point to the directory containing ffmpeg binary
            // Or directly to the ffmpeg binary
            config.ffmpeg.ffmpeg_path = path.clone();
            // Try to derive ffprobe path from same directory
            let p = std::path::Path::new(&path);
            if let Some(dir) = p.parent() {
                #[cfg(target_os = "windows")]
                let ffprobe = dir.join("ffprobe.exe");
                #[cfg(not(target_os = "windows"))]
                let ffprobe = dir.join("ffprobe");
                if ffprobe.exists() {
                    config.ffmpeg.ffprobe_path = ffprobe.to_string_lossy().to_string();
                }
            }
        }
        "danmaku-factory" => {
            config.tools.danmaku_factory_path = path;
        }
        _ => {
            return Err(format!(
                "Custom path not supported for '{}', use settings directly",
                dep_id
            ));
        }
    }

    config
        .save(&state.config_dir)
        .map_err(|e| format!("Failed to save config: {}", e))?;

    re_detect_tool_paths(&dep_id, &state);

    Ok(())
}

/// Set HTTP proxy for dependency downloads (saves to config.toml and updates HTTP client)
#[tauri::command]
pub async fn set_deps_proxy(proxy_url: String, state: State<'_, AppState>) -> Result<(), String> {
    // Save to config.toml
    let mut config = state.config.write().map_err(|e| e.to_string())?;
    config.network.proxy_url = proxy_url.clone();
    config
        .save(&state.config_dir)
        .map_err(|e| format!("Failed to save config: {}", e))?;

    // Update HTTP client immediately
    let proxy = if proxy_url.is_empty() {
        None
    } else {
        Some(proxy_url.as_str())
    };
    state.dep_manager.update_proxy(proxy);

    tracing::info!(
        "Deps proxy updated: {}",
        if proxy_url.is_empty() {
            "(disabled)"
        } else {
            &proxy_url
        }
    );
    Ok(())
}

/// Get current proxy URL from config
#[tauri::command]
pub async fn get_deps_proxy(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config.read().map_err(|e| e.to_string())?;
    Ok(config.network.proxy_url.clone())
}

/// Open the dependency installation directory in file manager
#[tauri::command]
pub async fn reveal_dep_dir(dep_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let dep_dir = state.dep_manager.deps_dir().join(&dep_id);
    if !dep_dir.exists() {
        return Err(format!("依赖 '{}' 未安装", dep_id));
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let path_str = dep_dir.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    Ok(())
}
