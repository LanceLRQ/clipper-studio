use tauri::{AppHandle, State};

use crate::deps::registry::DependencyStatus;
use crate::deps::config_overrides_from_app_config;
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
    state.dep_manager.check_dep(&dep_id, &overrides, &state.bin_dir)
}

/// Install a dependency (download + extract + verify)
#[tauri::command]
pub async fn install_dep(
    dep_id: String,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.dep_manager.install_dep(&dep_id, &app_handle).await
}

/// Uninstall a dependency
#[tauri::command]
pub async fn uninstall_dep(
    dep_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.dep_manager.uninstall_dep(&dep_id)
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

    Ok(())
}

/// Open the dependency installation directory in file manager
#[tauri::command]
pub async fn reveal_dep_dir(
    dep_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let dep_dir = state.dep_manager.deps_dir().join(&dep_id);
    if !dep_dir.exists() {
        return Err(format!("依赖 '{}' 未安装", dep_id));
    }

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
