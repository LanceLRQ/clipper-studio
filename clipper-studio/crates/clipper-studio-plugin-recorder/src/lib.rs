//! Builtin BilibiliRecorder plugin
//!
//! This plugin connects to a running BilibiliRecorder instance via HTTP API.

use async_trait::async_trait;
use clipper_studio_plugin_core::{
    PluginError, PluginFrontend, PluginInstance, PluginManifest, PluginType, Transport,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// BilibiliRecorder plugin instance
pub struct BilibiliRecorderPlugin {
    manifest: PluginManifest,
    /// HTTP client for making requests to BilibiliRecorder
    client: reqwest::Client,
    /// Cached config (base_url, api_key, etc.)
    config: RwLock<BilibiliRecorderConfig>,
}

/// Configuration for BilibiliRecorder connection
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BilibiliRecorderConfig {
    pub api_url: String,
    pub api_key: String,
    pub basic_user: String,
    pub basic_pass: String,
}

#[async_trait]
impl PluginInstance for BilibiliRecorderPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn initialize(&self) -> Result<(), PluginError> {
        tracing::info!("Initializing BilibiliRecorder plugin");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PluginError> {
        tracing::info!("Shutting down BilibiliRecorder plugin");
        Ok(())
    }

    async fn handle_request(
        &self,
        action: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        // Extract config from payload (for backward compatibility with frontend config flow)
        // Frontend sends base_url (not api_url), so we check both for compatibility
        let internal_config = self.config.read().await;

        // base_url is what frontend sends; also check api_url for direct API callers
        let api_url = payload
            .get("base_url")
            .or_else(|| payload.get("api_url"))
            .and_then(|v| v.as_str())
            .unwrap_or(&internal_config.api_url)
            .to_string();
        let api_key = payload
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or(&internal_config.api_key)
            .to_string();
        let basic_user = payload
            .get("basic_user")
            .and_then(|v| v.as_str())
            .unwrap_or(&internal_config.basic_user)
            .to_string();
        let basic_pass = payload
            .get("basic_pass")
            .and_then(|v| v.as_str())
            .unwrap_or(&internal_config.basic_pass)
            .to_string();

        let config = BilibiliRecorderConfig {
            api_url,
            api_key,
            basic_user,
            basic_pass,
        };

        match action {
            "status" => {
                self.call_bilibili(&config, "/status", payload)
                    .await
            }
            "sync_files" => {
                self.call_bilibili(&config, "/sync_files", payload)
                    .await
            }
            "get_rooms" => {
                self.call_bilibili(&config, "/rooms", payload)
                    .await
            }
            "get_config" => {
                // Return current config (without secrets)
                Ok(serde_json::json!({
                    "api_url": internal_config.api_url,
                    "has_api_key": !internal_config.api_key.is_empty(),
                    "has_basic_auth": !internal_config.basic_user.is_empty(),
                }))
            }
            "set_config" => {
                // Update internal config (for persistent storage in plugin itself)
                if let Some(obj) = payload.as_object() {
                    let mut cfg = self.config.write().await;
                    if let Some(v) = obj.get("api_url").and_then(|v| v.as_str()) {
                        cfg.api_url = v.to_string();
                    }
                    if let Some(v) = obj.get("api_key").and_then(|v| v.as_str()) {
                        cfg.api_key = v.to_string();
                    }
                    if let Some(v) = obj.get("basic_user").and_then(|v| v.as_str()) {
                        cfg.basic_user = v.to_string();
                    }
                    if let Some(v) = obj.get("basic_pass").and_then(|v| v.as_str()) {
                        cfg.basic_pass = v.to_string();
                    }
                }
                Ok(serde_json::json!({"ok": true}))
            }
            _ => Err(PluginError::UnsupportedAction(action.to_string())),
        }
    }
}

impl BilibiliRecorderPlugin {
    /// Create a new BilibiliRecorder plugin instance
    pub fn new() -> Self {
        let manifest = PluginManifest {
            id: "builtin.recorder.bilibili".to_string(),
            name: "BilibiliRecorder".to_string(),
            plugin_type: PluginType::Recorder,
            version: "1.0.0".to_string(),
            api_version: 1,
            transport: Transport::Builtin,
            managed: false,
            singleton: true,
            startup: None,
            executable: None,
            health_endpoint: None,
            port: None,
            config_schema: [
                ("api_url".to_string(), serde_json::json!({
                    "type": "string",
                    "default": "http://127.0.0.1:2007",
                    "description": "录播姬 HTTP API 地址"
                })),
                ("api_key".to_string(), serde_json::json!({
                    "type": "string",
                    "default": "",
                    "description": "录播姬 API 密钥（可选）"
                })),
                ("basic_user".to_string(), serde_json::json!({
                    "type": "string",
                    "default": "",
                    "description": "HTTP Basic 认证用户名（留空关闭）"
                })),
                ("basic_pass".to_string(), serde_json::json!({
                    "type": "string",
                    "default": "",
                    "description": "HTTP Basic 认证密码"
                })),
            ]
            .into_iter()
            .collect(),
            dependencies: vec![],
            conflicts: vec![],
            description: Some(
                "对接 BilibiliRecorder 录播姬，获取房间列表、同步录制文件".to_string(),
            ),
            frontend: Some(PluginFrontend {
                entry: "".to_string(), // No separate UI bundle needed, uses built-in panel
                target: "recorder".to_string(),
            }),
        };

        Self {
            manifest,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            config: RwLock::new(BilibiliRecorderConfig {
                api_url: "http://127.0.0.1:2007".to_string(),
                api_key: String::new(),
                basic_user: String::new(),
                basic_pass: String::new(),
            }),
        }
    }

    /// Make an HTTP call to BilibiliRecorder
    async fn call_bilibili(
        &self,
        config: &BilibiliRecorderConfig,
        endpoint: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let url = format!(
            "{}{}",
            config.api_url.trim_end_matches('/'),
            endpoint
        );

        let mut req = self.client.post(&url).json(&payload);

        // Add API Key header
        if !config.api_key.is_empty() {
            req = req.header("X-API-Key", &config.api_key);
        }

        // Add Basic Auth
        if !config.basic_user.is_empty() {
            let credentials = Self::base64_encode(&format!(
                "{}:{}",
                config.basic_user, config.basic_pass
            ));
            req = req.header("Authorization", format!("Basic {}", credentials));
        }

        let resp = req.send().await.map_err(|e| {
            tracing::error!("Failed to call BilibiliRecorder: {}", e);
            PluginError::Transport(format!("HTTP request failed: {}", e))
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(PluginError::Transport(format!(
                "HTTP {} error: {}",
                status, body
            )));
        }

        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| PluginError::Transport(format!("Failed to parse response: {}", e)))
    }

    /// Simple base64 encoding (standard alphabet, no padding)
    fn base64_encode(input: &str) -> String {
        const ALPHABET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = input.as_bytes();
        let mut result = String::new();
        for chunk in bytes.chunks(3) {
            let b = match chunk.len() {
                1 => [chunk[0], 0, 0],
                2 => [chunk[0], chunk[1], 0],
                _ => [chunk[0], chunk[1], chunk[2]],
            };
            result.push(ALPHABET[(b[0] >> 2) as usize] as char);
            result.push(
                ALPHABET[((b[0] & 0x03) << 4 | b[1] >> 4) as usize] as char,
            );
            match chunk.len() {
                1 => result.push_str("=="),
                2 => {
                    result.push(
                        ALPHABET[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize] as char,
                    );
                    result.push('=');
                }
                _ => {
                    result.push(
                        ALPHABET[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize] as char,
                    );
                    result.push(ALPHABET[(b[2] & 0x3f) as usize] as char);
                }
            }
        }
        result
    }
}

// ===== PluginBuilder =====

pub struct BilibiliRecorderPluginBuilder;

impl BilibiliRecorderPluginBuilder {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BilibiliRecorderPluginBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl clipper_studio_plugin_core::PluginBuilder for BilibiliRecorderPluginBuilder {
    fn id(&self) -> &'static str {
        "builtin.recorder.bilibili"
    }

    fn build(&self) -> Result<Box<dyn PluginInstance>, PluginError> {
        Ok(Box::new(BilibiliRecorderPlugin::new()))
    }
}
