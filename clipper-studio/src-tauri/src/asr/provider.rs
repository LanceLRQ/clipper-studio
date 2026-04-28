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
        segments: Vec<RawASRSegment>,
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

/// A single word with timestamp from ASR (character-level for Chinese)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Raw ASR segment before splitting, includes optional word-level timestamps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawASRSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Word-level timestamps, present when align_enabled=true in ASR service
    pub words: Option<Vec<ASRWord>>,
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

    /// Cancel a remote ASR task. Best-effort; errors are logged but not fatal.
    async fn cancel(&self, task_id: &str) -> Result<(), String>;

    /// Health check
    async fn health(&self) -> Result<ASRHealthInfo, String>;

    /// Provider identifier
    fn provider_id(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============== ASRTaskStatus 序列化 ==============

    #[test]
    fn test_status_pending_serializes_with_tag() {
        let s = ASRTaskStatus::Pending;
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["status"], "pending");
    }

    #[test]
    fn test_status_processing_includes_progress() {
        let s = ASRTaskStatus::Processing { progress: 0.42 };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["status"], "processing");
        assert!((json["progress"].as_f64().unwrap() - 0.42).abs() < 1e-9);
    }

    #[test]
    fn test_status_completed_carries_segments() {
        let segments = vec![RawASRSegment {
            start: 0.0,
            end: 1.5,
            text: "你好".to_string(),
            words: None,
        }];
        let s = ASRTaskStatus::Completed { segments };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["status"], "completed");
        assert!(json["segments"].is_array());
        assert_eq!(json["segments"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_status_retryable_error_with_count() {
        let s = ASRTaskStatus::RetryableError {
            error: "timeout".to_string(),
            retry_count: 3,
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["status"], "retryable_error");
        assert_eq!(json["error"], "timeout");
        assert_eq!(json["retry_count"], 3);
    }

    #[test]
    fn test_status_permanent_error() {
        let s = ASRTaskStatus::PermanentError {
            error: "format unsupported".to_string(),
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["status"], "permanent_error");
        assert_eq!(json["error"], "format unsupported");
    }

    #[test]
    fn test_status_round_trip_pending() {
        let original = ASRTaskStatus::Pending;
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ASRTaskStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ASRTaskStatus::Pending));
    }

    #[test]
    fn test_status_round_trip_processing() {
        let original = ASRTaskStatus::Processing { progress: 0.75 };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ASRTaskStatus = serde_json::from_str(&json).unwrap();
        match parsed {
            ASRTaskStatus::Processing { progress } => {
                assert!((progress - 0.75).abs() < 1e-9);
            }
            other => panic!("expected Processing, got {:?}", other),
        }
    }

    #[test]
    fn test_status_round_trip_completed() {
        let original = ASRTaskStatus::Completed {
            segments: vec![RawASRSegment {
                start: 1.0,
                end: 2.0,
                text: "测试".to_string(),
                words: Some(vec![ASRWord {
                    text: "测".to_string(),
                    start: 1.0,
                    end: 1.5,
                }]),
            }],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ASRTaskStatus = serde_json::from_str(&json).unwrap();
        match parsed {
            ASRTaskStatus::Completed { segments } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].text, "测试");
                assert_eq!(segments[0].words.as_ref().unwrap().len(), 1);
            }
            other => panic!("expected Completed, got {:?}", other),
        }
    }

    // ============== ASRSegment / RawASRSegment ==============

    #[test]
    fn test_asr_segment_round_trip() {
        let original = ASRSegment {
            start: 1.5,
            end: 3.5,
            text: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ASRSegment = serde_json::from_str(&json).unwrap();
        assert!((parsed.start - 1.5).abs() < 1e-9);
        assert!((parsed.end - 3.5).abs() < 1e-9);
        assert_eq!(parsed.text, "Hello world");
    }

    #[test]
    fn test_raw_segment_words_optional() {
        let with_words = RawASRSegment {
            start: 0.0,
            end: 1.0,
            text: "x".to_string(),
            words: Some(vec![]),
        };
        let without_words = RawASRSegment {
            start: 0.0,
            end: 1.0,
            text: "y".to_string(),
            words: None,
        };

        let j1 = serde_json::to_value(&with_words).unwrap();
        let j2 = serde_json::to_value(&without_words).unwrap();

        assert!(j1["words"].is_array());
        assert!(j2["words"].is_null(), "words=None 应序列化为 null");
    }

    #[test]
    fn test_raw_segment_with_unicode_text() {
        let original = RawASRSegment {
            start: 0.0,
            end: 1.0,
            text: "你好世界 🌍".to_string(),
            words: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: RawASRSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "你好世界 🌍");
    }

    // ============== ASRWord ==============

    #[test]
    fn test_asr_word_round_trip() {
        let original = ASRWord {
            text: "字".to_string(),
            start: 0.5,
            end: 0.7,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ASRWord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "字");
        assert!((parsed.start - 0.5).abs() < 1e-9);
        assert!((parsed.end - 0.7).abs() < 1e-9);
    }

    // ============== ASRHealthInfo ==============

    #[test]
    fn test_health_info_with_optional_fields() {
        let info = ASRHealthInfo {
            status: "ok".to_string(),
            device: Some("cuda".to_string()),
            model_size: Some("large".to_string()),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["device"], "cuda");
        assert_eq!(json["model_size"], "large");
    }

    #[test]
    fn test_health_info_with_none_fields_round_trip() {
        let original = ASRHealthInfo {
            status: "degraded".to_string(),
            device: None,
            model_size: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ASRHealthInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, "degraded");
        assert!(parsed.device.is_none());
        assert!(parsed.model_size.is_none());
    }

    #[test]
    fn test_status_unknown_tag_fails_to_deserialize() {
        let bogus = r#"{"status":"flying"}"#;
        let result: Result<ASRTaskStatus, _> = serde_json::from_str(bogus);
        assert!(result.is_err(), "未知 tag 应反序列化失败");
    }
}
