use std::error::Error;
use std::path::Path;

use super::provider::{ASRHealthInfo, ASRProvider, ASRTaskStatus, ASRWord, RawASRSegment};

/// Local ASR provider using qwen3-asr-service (HTTP API on localhost)
pub struct LocalASRProvider {
    client: reqwest::Client,
    base_url: String,
}

impl LocalASRProvider {
    /// Create a local provider.
    ///
    /// `host` is the bind host of the local service. For a wildcard bind
    /// ("0.0.0.0"), connections are made via 127.0.0.1. Empty host also
    /// falls back to 127.0.0.1.
    pub fn new(host: &str, port: u16) -> Self {
        let connect_host = if host.is_empty() || host == "0.0.0.0" {
            "127.0.0.1"
        } else {
            host
        };
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .no_proxy()
                .build()
                .unwrap_or_default(),
            base_url: format!("http://{}:{}", connect_host, port),
        }
    }

    pub fn with_url(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .no_proxy()
                .build()
                .unwrap_or_default(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait::async_trait]
impl ASRProvider for LocalASRProvider {
    async fn submit(&self, file_path: &Path, language: Option<&str>) -> Result<String, String> {
        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let file_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or("audio.wav".to_string());

        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")
            .map_err(|e| e.to_string())?;

        let mut form = reqwest::multipart::Form::new().part("file", file_part);
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let resp = self
            .client
            .post(format!("{}/v1/asr", self.base_url))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("ASR submit failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("ASR submit HTTP {}: {}", status, body));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        json.get("task_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or("No task_id in response".to_string())
    }

    async fn query(&self, task_id: &str) -> Result<ASRTaskStatus, String> {
        let resp = self
            .client
            .get(format!("{}/v1/tasks/{}", self.base_url, task_id))
            .send()
            .await
            .map_err(|e| format!("ASR query failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("ASR query HTTP {}", resp.status()));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let status = json
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        match status {
            "pending" => Ok(ASRTaskStatus::Pending),
            "processing" => {
                let progress = json.get("progress").and_then(|p| p.as_f64()).unwrap_or(0.0);
                Ok(ASRTaskStatus::Processing { progress })
            }
            "completed" => {
                let segments = parse_segments(&json);
                Ok(ASRTaskStatus::Completed { segments })
            }
            "failed" => {
                let error = json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                // Classify error type
                if is_retryable_error(&error) {
                    Ok(ASRTaskStatus::RetryableError {
                        error,
                        retry_count: 0,
                    })
                } else {
                    Ok(ASRTaskStatus::PermanentError { error })
                }
            }
            _ => Err(format!("Unknown ASR status: {}", status)),
        }
    }

    async fn cancel(&self, task_id: &str) -> Result<(), String> {
        let resp = self
            .client
            .delete(format!("{}/v1/tasks/{}", self.base_url, task_id))
            .send()
            .await
            .map_err(|e| format!("ASR cancel failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("ASR cancel HTTP {}: {}", status, body));
        }

        Ok(())
    }

    async fn health(&self) -> Result<ASRHealthInfo, String> {
        let url = format!("{}/v1/health", self.base_url);
        tracing::info!("[ASR] Health check request: GET {}", url);

        let health_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap_or_default();

        let resp = match health_client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "[ASR] Health check connection failed for {}: {} (is_connect: {:?}, is_timeout: {:?}, source: {:?})",
                    url,
                    e,
                    e.is_connect(),
                    e.is_timeout(),
                    e.source()
                );
                return Err(format!("Health check failed: {}", e));
            }
        };

        let status = resp.status();
        tracing::info!("[ASR] Health check response: HTTP {}", status);

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(
                "[ASR] Health check non-success: HTTP {} body={}",
                status,
                body
            );
            return Err(format!("ASR service not healthy (HTTP {})", status));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| {
            tracing::warn!("[ASR] Health check JSON parse error: {}", e);
            e.to_string()
        })?;
        tracing::info!("[ASR] Health check response body: {}", json);
        Ok(ASRHealthInfo {
            status: json
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string(),
            device: json
                .get("device")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
            model_size: json
                .get("model_size")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
        })
    }

    fn provider_id(&self) -> &str {
        "local"
    }
}

/// Parse ASR segments from the response JSON, including word-level timestamps
fn parse_segments(json: &serde_json::Value) -> Vec<RawASRSegment> {
    json.get("result")
        .and_then(|r| r.get("segments"))
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|seg| {
                    let start = seg.get("start")?.as_f64()?;
                    let end = seg.get("end")?.as_f64()?;
                    let text = seg.get("text")?.as_str()?.to_string();
                    let words = seg.get("words").and_then(|w| w.as_array()).map(|warr| {
                        warr.iter()
                            .filter_map(|w| {
                                Some(ASRWord {
                                    text: w.get("text")?.as_str()?.to_string(),
                                    start: w.get("start")?.as_f64()?,
                                    end: w.get("end")?.as_f64()?,
                                })
                            })
                            .collect()
                    });
                    Some(RawASRSegment {
                        start,
                        end,
                        text,
                        words,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Classify whether an error is retryable
fn is_retryable_error(error: &str) -> bool {
    let retryable_keywords = [
        "timeout",
        "connection",
        "busy",
        "temporarily",
        "503",
        "429",
        "network",
    ];
    let lower = error.to_lowercase();
    retryable_keywords.iter().any(|kw| lower.contains(kw))
}
