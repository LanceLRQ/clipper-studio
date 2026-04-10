use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;

use super::provider::ASRHealthInfo;

/// Maximum log lines retained
const MAX_LOG_LINES: usize = 500;

/// Health check interval during startup (seconds)
const HEALTH_CHECK_INTERVAL_SECS: u64 = 3;

/// Maximum health check attempts (3s * 40 = 2 minutes)
const MAX_HEALTH_CHECK_ATTEMPTS: u32 = 40;

// ==================== Types ====================

/// ASR service lifecycle status
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ASRServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error { message: String },
}

/// Startup configuration for the ASR service
#[derive(Debug, Clone)]
pub struct ASRStartConfig {
    pub port: u16,
    pub device: String,
    pub model_size: String,
    pub enable_align: bool,
    pub enable_punc: bool,
    pub model_source: String,
    pub max_segment: u32,
    pub host: String,
}

impl Default for ASRStartConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            device: "auto".to_string(),
            model_size: "auto".to_string(),
            enable_align: true,
            enable_punc: true,
            model_source: "modelscope".to_string(),
            max_segment: 5,
            host: "127.0.0.1".to_string(),
        }
    }
}

/// Result of validating an ASR service path
#[derive(Debug, Clone, Serialize)]
pub struct ASRPathValidation {
    pub valid: bool,
    pub has_venv: bool,
    pub has_main: bool,
    pub python_path: String,
}

/// Combined status info returned to frontend
#[derive(Debug, Clone, Serialize)]
pub struct ASRServiceStatusInfo {
    #[serde(flatten)]
    pub status: ASRServiceStatus,
    pub health_info: Option<ASRHealthInfo>,
}

// ==================== ASRServiceManager ====================

/// Manages the lifecycle of a local qwen3-asr-service process
pub struct ASRServiceManager {
    child: Mutex<Option<Child>>,
    status: Mutex<ASRServiceStatus>,
    logs: Mutex<VecDeque<String>>,
    health_info: Mutex<Option<ASRHealthInfo>>,
}

impl ASRServiceManager {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            status: Mutex::new(ASRServiceStatus::Stopped),
            logs: Mutex::new(VecDeque::new()),
            health_info: Mutex::new(None),
        }
    }

    /// Validate that a given directory contains a valid qwen3-asr-service installation
    pub fn validate_path(base_dir: &Path) -> ASRPathValidation {
        let python_path = get_python_path(base_dir);
        let main_path = base_dir.join("asr-service").join("app").join("main.py");

        let has_venv = python_path.exists();
        let has_main = main_path.exists();

        ASRPathValidation {
            valid: has_venv && has_main,
            has_venv,
            has_main,
            python_path: python_path.to_string_lossy().to_string(),
        }
    }

    /// Start the ASR service with the given configuration.
    /// `self_arc` is needed so the background health-check task can update manager state.
    pub async fn start(
        self: &Arc<Self>,
        base_dir: &Path,
        config: ASRStartConfig,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        // Check current status
        {
            let status = self.status.lock().map_err(|e| e.to_string())?;
            match &*status {
                ASRServiceStatus::Running => return Ok(()),
                ASRServiceStatus::Starting => {
                    return Err("ASR 服务正在启动中，请等待".to_string());
                }
                ASRServiceStatus::Stopping => {
                    return Err("ASR 服务正在停止中，请等待".to_string());
                }
                _ => {}
            }
        }

        // Validate path
        let validation = Self::validate_path(base_dir);
        if !validation.valid {
            let msg = if !validation.has_venv {
                "未找到 Python 虚拟环境（venv）"
            } else {
                "未找到 app/main.py 入口文件"
            };
            return Err(format!("ASR 服务路径无效：{}", msg));
        }

        let python_path = get_python_path(base_dir);
        let working_dir = base_dir.join("asr-service");

        // Build command arguments
        let mut args: Vec<String> = vec![
            "-m".to_string(),
            "app.main".to_string(),
            "--host".to_string(),
            config.host.clone(),
            "--port".to_string(),
            config.port.to_string(),
            "--device".to_string(),
            config.device.clone(),
            "--model-source".to_string(),
            config.model_source.clone(),
            "--max-segment".to_string(),
            config.max_segment.to_string(),
        ];

        // Model size (only pass if not "auto")
        if config.model_size != "auto" {
            args.push("--model-size".to_string());
            args.push(config.model_size.clone());
        }

        // Alignment
        if config.enable_align {
            args.push("--enable-align".to_string());
        } else {
            args.push("--no-align".to_string());
        }

        // Punctuation
        if config.enable_punc {
            args.push("--use-punc".to_string());
        }

        tracing::info!(
            "Starting ASR service: {} {}",
            python_path.display(),
            args.join(" ")
        );

        // Clear previous state
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
        if let Ok(mut hi) = self.health_info.lock() {
            *hi = None;
        }

        // Spawn process
        let mut child = tokio::process::Command::new(&python_path)
            .args(&args)
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("启动 ASR 服务失败：{}", e))?;

        // Capture stdout in background
        if let Some(stdout) = child.stdout.take() {
            let mgr = Arc::clone(self);
            let app_h = app_handle.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!("[asr-service] {}", line);
                    mgr.add_log(&line);
                    let _ = app_h.emit("asr-service-log", &line);
                }
            });
        }

        // Capture stderr in background
        if let Some(stderr) = child.stderr.take() {
            let mgr = Arc::clone(self);
            let app_h = app_handle.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("[asr-service] stderr: {}", line);
                    mgr.add_log(&format!("[stderr] {}", line));
                    let _ = app_h.emit("asr-service-log", &line);
                }
            });
        }

        // Store child process
        if let Ok(mut guard) = self.child.lock() {
            *guard = Some(child);
        }

        // Set status to Starting and notify frontend
        self.set_status(ASRServiceStatus::Starting);
        self.emit_status(&app_handle);

        // Spawn background health check polling task
        let health_url = format!("http://{}:{}/v1/health", config.host, config.port);
        let mgr = Arc::clone(self);
        let app_h = app_handle.clone();

        tokio::spawn(async move {
            // Brief initial wait for process to initialize
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            for attempt in 0..MAX_HEALTH_CHECK_ATTEMPTS {
                // Check if process is still alive
                if !mgr.is_running() {
                    let msg = "ASR 服务进程已退出，请查看日志了解原因".to_string();
                    tracing::error!("{}", msg);
                    mgr.set_status(ASRServiceStatus::Error {
                        message: msg,
                    });
                    mgr.emit_status(&app_h);
                    return;
                }

                // Check if someone called stop() while we're polling
                if mgr.status() == ASRServiceStatus::Stopping
                    || mgr.status() == ASRServiceStatus::Stopped
                {
                    return;
                }

                match client.get(&health_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            let health = ASRHealthInfo {
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
                            };

                            tracing::info!(
                                "ASR service healthy (attempt {}/{}): device={:?}, model={:?}",
                                attempt + 1,
                                MAX_HEALTH_CHECK_ATTEMPTS,
                                health.device,
                                health.model_size,
                            );

                            // Update manager state
                            if let Ok(mut hi) = mgr.health_info.lock() {
                                *hi = Some(health);
                            }
                            mgr.set_status(ASRServiceStatus::Running);
                            mgr.emit_status(&app_h);
                            return;
                        }
                    }
                    Ok(resp) => {
                        tracing::debug!(
                            "ASR health check attempt {}/{}: HTTP {}",
                            attempt + 1,
                            MAX_HEALTH_CHECK_ATTEMPTS,
                            resp.status(),
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            "ASR health check attempt {}/{}: {}",
                            attempt + 1,
                            MAX_HEALTH_CHECK_ATTEMPTS,
                            e,
                        );
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(
                    HEALTH_CHECK_INTERVAL_SECS,
                ))
                .await;
            }

            // Timeout: service did not become healthy
            let msg = "ASR 服务启动超时，模型加载可能失败，请查看日志".to_string();
            tracing::warn!("{}", msg);
            mgr.set_status(ASRServiceStatus::Error { message: msg });
            mgr.emit_status(&app_h);
        });

        Ok(())
    }

    /// Stop the ASR service
    pub async fn stop(&self) -> Result<(), String> {
        self.set_status(ASRServiceStatus::Stopping);

        // Extract child from lock before awaiting
        let child = self.child.lock().ok().and_then(|mut g| g.take());
        if let Some(mut child) = child {
            tracing::info!("Stopping ASR service...");
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        // Clear health info
        if let Ok(mut hi) = self.health_info.lock() {
            *hi = None;
        }

        self.set_status(ASRServiceStatus::Stopped);
        Ok(())
    }

    /// Get current service status
    pub fn status(&self) -> ASRServiceStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(ASRServiceStatus::Stopped)
    }

    /// Get current health info
    pub fn health_info(&self) -> Option<ASRHealthInfo> {
        self.health_info.lock().ok().and_then(|hi| hi.clone())
    }

    /// Get combined status info for frontend
    pub fn status_info(&self) -> ASRServiceStatusInfo {
        ASRServiceStatusInfo {
            status: self.status(),
            health_info: self.health_info(),
        }
    }

    /// Check if the service process is still running
    pub fn is_running(&self) -> bool {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(None) => return true,
                    Ok(Some(_)) => {
                        *guard = None;
                        return false;
                    }
                    Err(_) => return false,
                }
            }
        }
        false
    }

    /// Get recent log lines
    pub fn get_logs(&self, limit: usize) -> Vec<String> {
        self.logs
            .lock()
            .map(|logs| {
                let start = if logs.len() > limit {
                    logs.len() - limit
                } else {
                    0
                };
                logs.iter().skip(start).cloned().collect()
            })
            .unwrap_or_default()
    }

    /// Add a log line to the buffer
    fn add_log(&self, line: &str) {
        if let Ok(mut logs) = self.logs.lock() {
            if logs.len() >= MAX_LOG_LINES {
                logs.pop_front();
            }
            logs.push_back(line.to_string());
        }
    }

    /// Emit current status as a Tauri event
    fn emit_status(&self, app_handle: &AppHandle) {
        let _ = app_handle.emit("asr-service-status", self.status_info());
    }

    // Internal: set status
    fn set_status(&self, new_status: ASRServiceStatus) {
        if let Ok(mut s) = self.status.lock() {
            *s = new_status;
        }
    }
}

impl Drop for ASRServiceManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.start_kill();
            }
        }
    }
}

// ==================== Helpers ====================

/// Get the platform-appropriate python path within the service directory
fn get_python_path(base_dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        base_dir
            .join("asr-service")
            .join("venv")
            .join("Scripts")
            .join("python.exe")
    } else {
        base_dir
            .join("asr-service")
            .join("venv")
            .join("bin")
            .join("python3")
    }
}
