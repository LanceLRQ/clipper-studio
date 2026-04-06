use tauri::State;

use crate::AppState;
use crate::plugin::manager::PluginInfo;

/// Scan plugin directory and return all discovered plugins
#[tauri::command]
pub async fn scan_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<PluginInfo>, String> {
    state.plugin_manager.scan().await;
    Ok(state.plugin_manager.list().await)
}

/// List all discovered plugins
#[tauri::command]
pub async fn list_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<PluginInfo>, String> {
    Ok(state.plugin_manager.list().await)
}

/// Load a plugin (verify deps, create transport)
#[tauri::command]
pub async fn load_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state.plugin_manager.load(&plugin_id).await
}

/// Unload a plugin
#[tauri::command]
pub async fn unload_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state.plugin_manager.unload(&plugin_id).await
}

/// Start a managed service plugin
#[tauri::command]
pub async fn start_plugin_service(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state.plugin_manager.start_service(&plugin_id).await
}

/// Stop a managed service plugin
#[tauri::command]
pub async fn stop_plugin_service(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    state.plugin_manager.stop_service(&plugin_id).await
}
