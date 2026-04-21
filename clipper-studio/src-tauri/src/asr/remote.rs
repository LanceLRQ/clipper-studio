use std::path::Path;

use super::provider::{ASRHealthInfo, ASRProvider, ASRTaskStatus, ASRWord, RawASRSegment};

/// Remote ASR provider (external HTTP API with optional API key)
pub struct RemoteASRProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl RemoteASRProvider {
    pub fn new(base_url: &str, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .unwrap_or_default(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
}

#[async_trait::async_trait]
impl ASRProvider for RemoteASRProvider {
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

        let mut req = self.client.post(format!("{}/v1/asr", self.base_url));
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
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
        let mut req = self
            .client
            .get(format!("{}/v1/tasks/{}", self.base_url, task_id));
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
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
                let segments = json
                    .get("result")
                    .and_then(|r| r.get("segments"))
                    .and_then(|s| s.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|seg| {
                                let start = seg.get("start")?.as_f64()?;
                                let end = seg.get("end")?.as_f64()?;
                                let text = seg.get("text")?.as_str()?.to_string();
                                let words =
                                    seg.get("words").and_then(|w| w.as_array()).map(|warr| {
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
                    .unwrap_or_default();
                Ok(ASRTaskStatus::Completed { segments })
            }
            "failed" => {
                let error = json
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                Ok(ASRTaskStatus::PermanentError { error })
            }
            _ => Err(format!("Unknown ASR status: {}", status)),
        }
    }

    async fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut req = self
            .client
            .delete(format!("{}/v1/tasks/{}", self.base_url, task_id));
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
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
        // Use a short timeout for health checks (5s)
        let health_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let mut req = health_client.get(format!("{}/v1/health", self.base_url));
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Health check failed: {}", e))?;

        if !resp.status().is_success() {
            return Err("ASR service not healthy".to_string());
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
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
        "remote"
    }
}
