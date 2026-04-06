use std::path::Path;

use serde::{Deserialize, Serialize};

/// ASR task status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ASRTaskStatus {
    Pending,
    Processing {
        progress: f64,
    },
    Completed {
        segments: Vec<ASRSegment>,
    },
    /// Temporary error (network timeout, service busy) → retryable
    RetryableError {
        error: String,
        retry_count: u32,
    },
    /// Permanent error (unsupported format, decode failure) → not retryable
    PermanentError {
        error: String,
    },
}

/// A single ASR segment (file-relative time in seconds)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRSegment {
    /// Start time in seconds (relative to file start)
    pub start: f64,
    /// End time in seconds (relative to file start)
    pub end: f64,
    /// Recognized text
    pub text: String,
}

/// ASR health info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRHealthInfo {
    pub status: String,
    pub device: Option<String>,
    pub model_size: Option<String>,
}

/// Unified ASR provider trait
#[async_trait::async_trait]
pub trait ASRProvider: Send + Sync {
    /// Submit an audio/video file for recognition.
    /// Returns a remote task ID for polling.
    async fn submit(&self, file_path: &Path, language: Option<&str>) -> Result<String, String>;

    /// Query task status and results.
    async fn query(&self, task_id: &str) -> Result<ASRTaskStatus, String>;

    /// Health check
    async fn health(&self) -> Result<ASRHealthInfo, String>;

    /// Provider identifier
    fn provider_id(&self) -> &str;
}
