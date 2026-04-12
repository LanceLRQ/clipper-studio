use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::process::Child;

use super::provider::ASRHealthInfo;

/// Maximum log lines retained
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
            enable_punc: false,
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
    /// Windows: portable Python (bin/python/python.exe + non-empty lib/)
    /// macOS/Linux: venv (asr-service/venv/bin/python3)
    pub has_python_env: bool,
    /// asr-service/app/main.py exists
    pub has_main: bool,
    /// Resolved python executable path (informational)
    pub python_path: String,
    /// Current platform: "windows" | "macos" | "linux"
    pub platform: String,
    /// Human-readable hint about what's missing (for setup guidance)
    pub setup_hint: Option<String>,
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
        let asr_dir = base_dir.join("asr-service");
        let main_path = asr_dir.join("app").join("main.py");
        let has_main = main_path.exists();

        #[cfg(target_os = "windows")]
        {
            let python_exe = asr_dir.join("bin").join("python").join("python.exe");
            let lib_dir = asr_dir.join("lib");
            let has_portable_python = python_exe.exists()
                && lib_dir.is_dir()
                && std::fs::read_dir(&lib_dir)
                    .map(|mut d| d.next().is_some())
                    .unwrap_or(false);

            let valid = has_portable_python && has_main;
            let setup_hint = if !has_portable_python && !has_main {
                Some("请先运行 setup.bat 安装 Python 环境和依赖".to_string())
            } else if !has_portable_python {
                Some("未找到便携 Python 环境（bin/python/python.exe 或 lib/），请运行 setup.bat".to_string())
            } else if !has_main {
                Some("未找到入口文件 app/main.py，请检查服务目录".to_string())
            } else {
                None
            };

            ASRPathValidation {
                valid,
                has_python_env: has_portable_python,
                has_main,
                python_path: python_exe.to_string_lossy().to_string(),
                platform: "windows".to_string(),
                setup_hint,
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let python_path = asr_dir.join("venv").join("bin").join("python3");
            let has_venv = python_path.exists();

            let valid = has_venv && has_main;
            let setup_hint = if !has_venv && !has_main {
                Some("请在终端中运行 setup.sh 初始化 venv 和依赖".to_string())
            } else if !has_venv {
                Some("未找到 Python venv，请在终端中运行 setup.sh".to_string())
            } else if !has_main {
                Some("未找到入口文件 app/main.py，请检查服务目录".to_string())
            } else {
                None
            };

            let platform = if cfg!(target_os = "macos") {
                "macos"
            } else {
                "linux"
            };

            ASRPathValidation {
                valid,
                has_python_env: has_venv,
                has_main,
                python_path: python_path.to_string_lossy().to_string(),
                platform: platform.to_string(),
                setup_hint,
            }
        }
    }

    /// Start the ASR service by opening an external terminal window.
    /// The service is NOT managed as a child process; health is monitored via HTTP.
    pub async fn start_external(
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
            return Err(format!(
                "ASR 服务路径无效：{}",
                validation.setup_hint.unwrap_or_default()
            ));
        }

        let script_path = get_start_script_path(base_dir);
        if !script_path.exists() {
            return Err(format!("未找到启动脚本：{}", script_path.display()));
        }

        let working_dir = base_dir.join("asr-service");
        let args = build_start_script_args(&config);

        tracing::info!(
            "Starting ASR service via external terminal: {} {}",
            script_path.display(),
            args.join(" ")
        );

        // Clear previous state
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
        if let Ok(mut hi) = self.health_info.lock() {
            *hi = None;
        }

        // Launch external terminal
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "ASR Service"])
                .arg(&script_path)
                .args(&args)
                .current_dir(&working_dir)
                .spawn()
                .map_err(|e| format!("启动终端失败：{}", e))?;
        }

        #[cfg(target_os = "macos")]
        {
            let script_str = script_path.to_string_lossy().to_string();
            let args_str = args.join(" ");
            let osa_script = format!(
                "tell application \"Terminal\"\nactivate\ndo script \"cd '{}' && '{}' {}\"\nend tell",
                working_dir.to_string_lossy(),
                script_str,
                args_str,
            );
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(&osa_script)
                .spawn()
                .map_err(|e| format!("启动终端失败：{}", e))?;
        }

        #[cfg(target_os = "linux")]
        {
            let script_str = script_path.to_string_lossy().to_string();
            let args_str = args.join(" ");
            let cmd_str = format!(
                "cd '{}' && '{}' {} ; exec bash",
                working_dir.to_string_lossy(),
                script_str,
                args_str,
            );
            let terminals = ["gnome-terminal", "konsole", "xfce4-terminal", "xterm"];
            let mut launched = false;
            for term in &terminals {
                let result = match *term {
                    "gnome-terminal" | "xfce4-terminal" => std::process::Command::new(term)
                        .arg("--")
                        .arg("bash")
                        .arg("-c")
                        .arg(&cmd_str)
                        .spawn(),
                    _ => std::process::Command::new(term)
                        .arg("-e")
                        .arg("bash")
                        .arg("-c")
                        .arg(&cmd_str)
                        .spawn(),
                };
                if result.is_ok() {
                    launched = true;
                    break;
                }
            }
            if !launched {
                return Err("未找到可用的终端模拟器（已尝试 gnome-terminal/konsole/xfce4-terminal/xterm）".to_string());
            }
        }

        // Set status to Starting and notify frontend
        self.set_status(ASRServiceStatus::Starting);
        self.emit_status(&app_handle);

        // Spawn background health check polling task (external mode, no process check)
        let health_url = format!("http://{}:{}/v1/health", config.host, config.port);
        let mgr = Arc::clone(self);
        let app_h = app_handle.clone();

        tokio::spawn(async move {
            // Wait for process to initialize
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            for attempt in 0..MAX_HEALTH_CHECK_ATTEMPTS {
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
            let msg = "ASR 服务启动超时，请查看外部终端窗口了解原因".to_string();
            tracing::warn!("{}", msg);
            mgr.set_status(ASRServiceStatus::Error { message: msg });
            mgr.emit_status(&app_h);
        });

        Ok(())
    }

    /// Stop the ASR service
    /// For external terminal mode (no child process), only updates status.
    /// The user should close the terminal window manually.
    pub async fn stop(&self) -> Result<(), String> {
        self.set_status(ASRServiceStatus::Stopping);

        // Try to kill child process (works for internal process mode)
        let child = self.child.lock().ok().and_then(|mut g| g.take());
        if let Some(mut child) = child {
            tracing::info!("Stopping ASR service (child process)...");
            let _ = child.kill().await;
            let _ = child.wait().await;
        } else {
            tracing::info!("No child process (external terminal mode), updating status only");
        }

        // Clear health info
        if let Ok(mut hi) = self.health_info.lock() {
            *hi = None;
        }

        self.set_status(ASRServiceStatus::Stopped);
        Ok(())
    }

    /// Open an external terminal to run the setup script (interactive, needs user input).
    /// This does NOT manage the process lifecycle -- just launches it.
    pub fn open_setup_terminal(base_dir: &Path) -> Result<(), String> {
        let setup_script = get_setup_script_path(base_dir);
        if !setup_script.exists() {
            return Err(format!("未找到安装脚本：{}", setup_script.display()));
        }

        let working_dir = base_dir.join("asr-service");

        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "ASR Setup"])
                .arg(&setup_script)
                .current_dir(&working_dir)
                .spawn()
                .map_err(|e| format!("启动终端失败：{}", e))?;
        }

        #[cfg(target_os = "macos")]
        {
            let osa_script = format!(
                "tell application \"Terminal\"\nactivate\ndo script \"cd '{}' && bash '{}'\"\nend tell",
                working_dir.to_string_lossy(),
                setup_script.to_string_lossy(),
            );
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(&osa_script)
                .spawn()
                .map_err(|e| format!("启动终端失败：{}", e))?;
        }

        #[cfg(target_os = "linux")]
        {
            let cmd_str = format!(
                "cd '{}' && bash '{}' ; exec bash",
                working_dir.to_string_lossy(),
                setup_script.to_string_lossy(),
            );
            let terminals = ["gnome-terminal", "konsole", "xfce4-terminal", "xterm"];
            let mut launched = false;
            for term in &terminals {
                let result = match *term {
                    "gnome-terminal" | "xfce4-terminal" => std::process::Command::new(term)
                        .arg("--")
                        .arg("bash")
                        .arg("-c")
                        .arg(&cmd_str)
                        .spawn(),
                    _ => std::process::Command::new(term)
                        .arg("-e")
                        .arg("bash")
                        .arg("-c")
                        .arg(&cmd_str)
                        .spawn(),
                };
                if result.is_ok() {
                    launched = true;
                    break;
                }
            }
            if !launched {
                return Err("未找到可用的终端模拟器".to_string());
            }
        }

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
/// Get the start script path for the current platform
fn get_start_script_path(base_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        base_dir.join("asr-service").join("start.bat")
    }
    #[cfg(not(target_os = "windows"))]
    {
        base_dir.join("asr-service").join("start.sh")
    }
}

/// Get the setup script path for the current platform
fn get_setup_script_path(base_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        base_dir.join("asr-service").join("setup.bat")
    }
    #[cfg(not(target_os = "windows"))]
    {
        base_dir.join("asr-service").join("setup.sh")
    }
}

/// Build command-line arguments from ASRStartConfig for the start script
fn build_start_script_args(config: &ASRStartConfig) -> Vec<String> {
    let mut args: Vec<String> = vec![
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

    if config.model_size != "auto" {
        args.push("--model-size".to_string());
        args.push(config.model_size.clone());
    }

    if config.enable_align {
        args.push("--enable-align".to_string());
    } else {
        args.push("--no-align".to_string());
    }

    if config.enable_punc {
        args.push("--use-punc".to_string());
    }

    args
}
