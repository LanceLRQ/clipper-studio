use tauri::State;

use crate::AppState;
use crate::plugin::manager::PluginInfo;
use crate::plugin::transport::PluginTransport;

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

/// Call a plugin action via its transport (generic RPC)
/// For HTTP plugins, if payload contains `base_url`, use that instead of the configured URL
#[tauri::command]
pub async fn call_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    action: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let base_url = payload.get("base_url").and_then(|v| v.as_str()).map(String::from);
    let mut actual_payload = payload.clone();
    if let Some(obj) = actual_payload.as_object_mut() {
        obj.remove("base_url");
    }

    let transport = state
        .plugin_manager
        .get_transport(&plugin_id)
        .await
        .ok_or(format!("Plugin '{}' not loaded", plugin_id))?;

    // For HTTP transport with custom base_url, create a one-off request
    if let Some(url) = base_url {
        use crate::plugin::transport::HttpTransport;
        let custom = HttpTransport::new(&url, None);
        custom.request(&action, actual_payload).await
    } else {
        transport.request(&action, payload).await
    }
}
