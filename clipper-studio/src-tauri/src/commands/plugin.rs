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

/// Scan plugin directory and return all discovered plugins
#[tauri::command]
pub async fn scan_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<PluginInfo>, String> {
    let plugin_dir = resolve_plugin_dir(&state).await;
    let _ = std::fs::create_dir_all(&plugin_dir);
    state.plugin_manager.scan(&plugin_dir).await;
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

/// Call a plugin action via HTTP (generic RPC with optional auth)
/// Supports: base_url, api_key, basic_user, basic_pass
#[tauri::command]
pub async fn call_plugin(
    _state: State<'_, AppState>,
    _plugin_id: String,
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
