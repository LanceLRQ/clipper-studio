use std::collections::HashSet;

use tauri::State;

use crate::AppState;
use crate::plugin::manager::PluginInfo;

/// Simple base64 encoding (standard alphabet, no padding)
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let b = match chunk.len() {
            1 => [chunk[0], 0, 0],
            2 => [chunk[0], chunk[1], 0],
            _ => [chunk[0], chunk[1], chunk[2]],
        };
        result.push(ALPHABET[(b[0] >> 2) as usize] as char);
        result.push(ALPHABET[((b[0] & 0x03) << 4 | b[1] >> 4) as usize] as char);
        match chunk.len() {
            1 => result.push_str("=="),
            2 => {
                result.push(ALPHABET[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize] as char);
                result.push('=');
            }
            _ => {
                result.push(ALPHABET[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize] as char);
                result.push(ALPHABET[(b[2] & 0x3f) as usize] as char);
            }
        }
    }
    result
}

/// Resolve plugin directory from settings or return default
async fn resolve_plugin_dir(state: &State<'_, AppState>) -> std::path::PathBuf {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'plugin_dir'".to_string(),
        ),
    )
    .await;

    if let Ok(Some(row)) = row {
        if let Ok(val) = row.try_get::<String>("", "value") {
            let path = std::path::PathBuf::from(&val);
            if path.is_absolute() {
                return path;
            }
            tracing::warn!("plugin_dir must be absolute, using default");
        }
    }

    state.config_dir.join("plugins")
}

/// Query all plugin IDs that are marked as enabled in settings_kv
async fn query_enabled_plugin_ids(state: &State<'_, AppState>) -> HashSet<String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT key FROM settings_kv WHERE key LIKE 'plugin:%:enabled' AND value = 'true'"
                .to_string(),
        ),
    )
    .await;

    let mut ids = HashSet::new();
    if let Ok(rows) = rows {
        for row in &rows {
            if let Ok(key) = row.try_get::<String>("", "key") {
                // key format: plugin:{id}:enabled
                if let Some(id) = key.strip_prefix("plugin:").and_then(|s| s.strip_suffix(":enabled")) {
                    ids.insert(id.to_string());
                }
            }
        }
    }
    ids
}

/// Scan plugin directory and return all discovered plugins
#[tauri::command]
pub async fn scan_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<PluginInfo>, String> {
    let plugin_dir = resolve_plugin_dir(&state).await;
    let _ = std::fs::create_dir_all(&plugin_dir);
    state.plugin_manager.scan(&plugin_dir).await;
    let enabled_ids = query_enabled_plugin_ids(&state).await;
    Ok(state.plugin_manager.list(&enabled_ids).await)
}

/// List all discovered plugins (builtin + external)
#[tauri::command]
pub async fn list_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<PluginInfo>, String> {
    let enabled_ids = query_enabled_plugin_ids(&state).await;

    // Builtin plugins from registry
    let builtin: Vec<PluginInfo> = state
        .plugin_registry
        .list_builtin_with_status(&enabled_ids)
        .into_iter()
        .map(|b| PluginInfo {
            id: b.id,
            name: b.name,
            version: b.version,
            plugin_type: b.plugin_type,
            transport: b.transport,
            managed: b.managed,
            status: b.status,
            description: b.description,
            has_config: b.has_config,
            enabled: b.enabled,
            config_schema: b.config_schema,
            frontend: b.frontend,
            dir: b.dir,
        })
        .collect();

    // External plugins from directory scan
    let external = state.plugin_manager.list(&enabled_ids).await;

    // Merge: external overrides builtin with same ID
    let mut result = builtin;
    for ext in external {
        if !result.iter().any(|p| p.id == ext.id) {
            result.push(ext);
        }
    }

    Ok(result)
}

/// Load a plugin (builtin or external)
#[tauri::command]
pub async fn load_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    // Check if it's a builtin plugin
    if state.plugin_registry.is_builtin(&plugin_id) {
        // Load builtin plugin via registry (transport stored in registry's instances)
        let _ = state.plugin_registry.load_builtin(&plugin_id).await?;
        // Note: builtin plugins don't need manager.register_transport or set_loaded
        // Their status is tracked via registry's instances HashMap
        Ok(())
    } else {
        // External plugin - use manager
        state.plugin_manager.load(&plugin_id).await
    }
}

/// Unload a plugin
#[tauri::command]
pub async fn unload_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    if state.plugin_registry.is_builtin(&plugin_id) {
        // Unload builtin plugin via registry
        state.plugin_registry.unload_builtin(&plugin_id).await?;
        let _ = state.plugin_manager.set_discovered(&plugin_id).await;
        Ok(())
    } else {
        // External plugin - use manager
        state.plugin_manager.unload(&plugin_id).await
    }
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

/// Call a plugin action (builtin or external)
/// For builtin plugins: calls handle_request directly via registry transport
/// For external plugins: makes HTTP request to the plugin endpoint
#[tauri::command]
pub async fn call_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    action: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Check if it's a builtin plugin first
    if state.plugin_registry.is_builtin(&plugin_id) {
        let transport = state
            .plugin_registry
            .get_transport(&plugin_id)
            .ok_or_else(|| format!("Builtin plugin '{}' not loaded", plugin_id))?;

        transport
            .request(&action, payload)
            .await
            .map_err(|e| e.to_string())
    } else {
        // External HTTP plugin - use existing HTTP logic
        call_plugin_http(action, payload).await
    }
}

/// Make HTTP call to external plugin (non-builtin)
async fn call_plugin_http(
    action: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Extract auth fields
    let base_url = payload
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("http://127.0.0.1:2007");
    let api_key = payload.get("api_key").and_then(|v| v.as_str());
    let basic_user = payload.get("basic_user").and_then(|v| v.as_str());
    let basic_pass = payload.get("basic_pass").and_then(|v| v.as_str());

    // Build per-call payload without auth fields
    let call_payload: serde_json::Value = {
        let mut map = serde_json::Map::new();
        if let Some(obj) = payload.as_object() {
            for (k, v) in obj {
                if !["base_url", "api_key", "basic_user", "basic_pass"].iter().any(|x| x == k) {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
        serde_json::Value::Object(map)
    };

    let url = format!("{}/{}", base_url.trim_end_matches('/'), action.trim_start_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let mut req = client.post(&url).json(&call_payload);

    // Add API Key header
    if let Some(key) = api_key {
        if !key.is_empty() {
            req = req.header("X-API-Key", key);
        }
    }

    // Add Basic Auth
    if let (Some(user), Some(pass)) = (basic_user, basic_pass) {
        if !user.is_empty() {
            let credentials = base64_encode(&format!("{}:{}", user, pass));
            req = req.header("Authorization", format!("Basic {}", credentials));
        }
    }

    let resp = req.send().await.map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {} error: {}", status, body));
    }

    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Get all config values for a plugin (reads from settings_kv with plugin:{id}: prefix)
#[tauri::command]
pub async fn get_plugin_config(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<std::collections::HashMap<String, String>, String> {
    let pattern = format!("plugin:{}:", plugin_id.replace('\'', "''"));
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT key, value FROM settings_kv WHERE key LIKE '{}%'", pattern),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut result = std::collections::HashMap::new();
    for row in &rows {
        if let (Ok(key), Ok(val)) = (
            row.try_get::<String>("", "key"),
            row.try_get::<String>("", "value"),
        ) {
            // Strip the prefix to get the config key name
            if let Some(config_key) = key.strip_prefix(&pattern) {
                result.insert(config_key.to_string(), val);
            }
        }
    }
    Ok(result)
}

/// Set a config value for a plugin (stores in settings_kv with plugin:{id}: prefix)
#[tauri::command]
pub async fn set_plugin_config(
    state: State<'_, AppState>,
    plugin_id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let full_key = format!(
        "plugin:{}:{}",
        plugin_id.replace('\'', "''"),
        key.replace('\'', "''")
    );
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('{}', '{}')",
            full_key,
            value.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Enable or disable a plugin (persists to settings_kv and loads/unloads accordingly)
#[tauri::command]
pub async fn set_plugin_enabled(
    state: State<'_, AppState>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let full_key = format!(
        "plugin:{}:enabled",
        plugin_id.replace('\'', "''")
    );

    // Persist enabled state to settings_kv
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('{}', '{}')",
            full_key,
            if enabled { "true" } else { "false" },
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Load or unload the plugin accordingly
    if enabled {
        if state.plugin_registry.is_builtin(&plugin_id) {
            let _ = state.plugin_registry.load_builtin(&plugin_id).await?;
        } else {
            state.plugin_manager.load(&plugin_id).await?;
        }
        tracing::info!("Plugin enabled and loaded: {}", plugin_id);
    } else {
        if state.plugin_registry.is_builtin(&plugin_id) {
            state.plugin_registry.unload_builtin(&plugin_id).await?;
            let _ = state.plugin_manager.set_discovered(&plugin_id).await;
        } else {
            state.plugin_manager.unload(&plugin_id).await?;
        }
        tracing::info!("Plugin disabled and unloaded: {}", plugin_id);
    }

    Ok(())
}

/// Auto-load all plugins that are marked as enabled in settings_kv
/// Called once during app startup
#[tauri::command]
pub async fn auto_load_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // First scan external plugins so they are discovered
    let plugin_dir = resolve_plugin_dir(&state).await;
    let _ = std::fs::create_dir_all(&plugin_dir);
    state.plugin_manager.scan(&plugin_dir).await;

    // Query enabled plugin IDs
    let enabled_ids = query_enabled_plugin_ids(&state).await;
    let mut loaded = Vec::new();

    for plugin_id in &enabled_ids {
        let result = if state.plugin_registry.is_builtin(plugin_id) {
            state.plugin_registry.load_builtin(plugin_id).await.map(|_| ())
        } else {
            state.plugin_manager.load(plugin_id).await
        };

        match result {
            Ok(()) => {
                tracing::info!("Auto-loaded enabled plugin: {}", plugin_id);
                loaded.push(plugin_id.clone());
            }
            Err(e) => {
                tracing::warn!("Failed to auto-load plugin {}: {}", plugin_id, e);
            }
        }
    }

    Ok(loaded)
}
