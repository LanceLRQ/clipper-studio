use std::path::PathBuf;
use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// Unified plugin communication abstraction.
///
/// Business code calls plugins through this trait without knowing
/// whether the plugin uses HTTP or stdio underneath.
#[async_trait::async_trait]
pub trait PluginTransport: Send + Sync {
    /// Send a request to the plugin and get a response
    async fn request(&self, action: &str, payload: Value) -> Result<Value, String>;

    /// Health check (for service plugins)
    async fn health(&self) -> Result<bool, String>;

    /// Shutdown the connection/process
    async fn shutdown(&self) -> Result<(), String>;
}

// ======================== HTTP Transport ========================

/// HTTP transport for service-level plugins (ASR, LLM, recorder, etc.)
pub struct HttpTransport {
    client: reqwest::Client,
    base_url: String,
    health_endpoint: String,
}

impl HttpTransport {
    pub fn new(base_url: &str, health_endpoint: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            base_url: base_url.trim_end_matches('/').to_string(),
            health_endpoint: health_endpoint.unwrap_or("/health").to_string(),
        }
    }
}

#[async_trait::async_trait]
impl PluginTransport for HttpTransport {
    async fn request(&self, action: &str, payload: Value) -> Result<Value, String> {
        let url = format!("{}/{}", self.base_url, action.trim_start_matches('/'));

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {} error: {}", status, body));
        }

        resp.json::<Value>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    async fn health(&self) -> Result<bool, String> {
        let url = format!("{}{}", self.base_url, self.health_endpoint);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    async fn shutdown(&self) -> Result<(), String> {
        // HTTP services are managed externally; nothing to do here
        Ok(())
    }
}

// ======================== Stdio Transport ========================

/// Stdio transport for tool-level plugins (danmaku converter, exporter, etc.)
///
/// Communication protocol: JSON-line per request/response.
/// Send: `{"action": "...", "payload": {...}}\n`
/// Recv: `{"result": {...}}\n` or `{"error": "..."}\n`
pub struct StdioTransport {
    executable: PathBuf,
    working_dir: PathBuf,
}

impl StdioTransport {
    pub fn new(executable: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            executable,
            working_dir,
        }
    }
}

#[async_trait::async_trait]
impl PluginTransport for StdioTransport {
    async fn request(&self, action: &str, payload: Value) -> Result<Value, String> {
        let request = serde_json::json!({
            "action": action,
            "payload": payload,
        });

        let mut child = Command::new(&self.executable)
            .current_dir(&self.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start plugin process: {}", e))?;

        // Write request to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Failed to write to plugin stdin: {}", e))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| e.to_string())?;
            // Close stdin to signal EOF
            drop(stdin);
        }

        // Read response from stdout
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture plugin stdout")?;
        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .await
            .map_err(|e| format!("Failed to read plugin response: {}", e))?;

        // Wait for process to exit
        let status = child
            .wait()
            .await
            .map_err(|e| format!("Plugin process error: {}", e))?;

        if response_line.trim().is_empty() {
            // No JSON response; check stderr
            if !status.success() {
                return Err(format!("Plugin process exited with code {:?}", status.code()));
            }
            return Ok(Value::Null);
        }

        let response: Value = serde_json::from_str(response_line.trim())
            .map_err(|e| format!("Failed to parse plugin response: {}", e))?;

        // Check for error in response
        if let Some(err) = response.get("error").and_then(|e| e.as_str()) {
            return Err(err.to_string());
        }

        Ok(response.get("result").cloned().unwrap_or(response))
    }

    async fn health(&self) -> Result<bool, String> {
        // Stdio plugins are ephemeral; "healthy" = executable exists
        Ok(self.executable.exists())
    }

    async fn shutdown(&self) -> Result<(), String> {
        // Stdio plugins are short-lived; nothing to clean up
        Ok(())
    }
}
