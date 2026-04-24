use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;

use super::docker;
use super::provider::ASRHealthInfo;

/// Maximum log lines retained in the ring buffer
const MAX_LOG_LINES: usize = 2000;

/// Health check interval during startup (seconds)
const HEALTH_CHECK_INTERVAL_SECS: u64 = 3;

/// Maximum health check attempts (3s * 40 = 2 minutes)
const MAX_HEALTH_CHECK_ATTEMPTS: u32 = 40;

/// Maximum slow-mode health check attempts (15s * 120 = 30 minutes)
const MAX_SLOW_MODE_ATTEMPTS: u32 = 120;

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

/// How the local ASR service is launched
#[derive(Debug, Clone)]
pub enum ASRLaunchMode {
    /// Run via local script (setup.sh/start.sh or setup.bat/start.bat)
    Native { base_dir: PathBuf },
    /// Run via docker container
    Docker {
        /// Full image tag, e.g. `lancelrq/qwen3-asr-service:latest-arm64`
        image: String,
        /// Host directory whose `models/` subdir is mounted to /app/models
        data_dir: PathBuf,
        /// Pass `--gpus all` to docker run
        use_gpu: bool,
        /// Optional `--platform` value (e.g. "linux/amd64")
        force_platform: Option<String>,
    },
}

/// Startup configuration for the ASR service
#[derive(Debug, Clone)]
pub struct ASRStartConfig {
    pub launch_mode: ASRLaunchMode,
    pub port: u16,
    pub device: String,
    pub model_size: String,
    pub enable_align: bool,
    pub enable_punc: bool,
    pub model_source: String,
    pub max_segment: u32,
    pub host: String,
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
    /// Which launch mode is currently active (if any)
    pub launch_kind: Option<&'static str>,
}

/// Error code returned when a conflicting container already exists.
/// Frontend should detect this exact string and prompt the user.
pub const ERR_CONTAINER_CONFLICT: &str = "DOCKER_CONTAINER_CONFLICT";

// ==================== ASRServiceManager ====================

/// Manages the lifecycle of a local qwen3-asr-service (native or docker)
pub struct ASRServiceManager {
    /// Native-mode child process (None in docker mode)
    child: Mutex<Option<Child>>,
    /// PID of the native child process, kept separately so we can still kill
    /// the process tree even after the Child handle is consumed by try_wait().
    child_pid: Mutex<Option<u32>>,
    status: Mutex<ASRServiceStatus>,
    logs: Mutex<VecDeque<String>>,
    health_info: Mutex<Option<ASRHealthInfo>>,
    /// Records the launch mode of the currently active (or last) run
    current_mode: Mutex<Option<ASRLaunchMode>>,
}

impl ASRServiceManager {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            child_pid: Mutex::new(None),
            status: Mutex::new(ASRServiceStatus::Stopped),
            logs: Mutex::new(VecDeque::new()),
            health_info: Mutex::new(None),
            current_mode: Mutex::new(None),
        }
    }

    /// Validate that a given directory contains a valid qwen3-asr-service installation.
    ///
    /// Accepts either the repo root (containing `asr-service/`) or the `asr-service/`
    /// subdirectory itself. If the user selected the subdirectory, it is normalized to
    /// the parent so that downstream code (start scripts, etc.) works uniformly.
    pub fn validate_path(raw_dir: &Path) -> ASRPathValidation {
        // Auto-detect: if the selected dir looks like the `asr-service` subdirectory
        // (contains `app/main.py` directly), treat its parent as the base_dir.
        let base_dir = if raw_dir.join("app").join("main.py").exists() && raw_dir.parent().is_some()
        {
            tracing::info!(
                "[ASR] validate_path: detected asr-service subdirectory, normalizing to parent: {}",
                raw_dir.parent().unwrap().display()
            );
            raw_dir.parent().unwrap()
        } else {
            raw_dir
        };

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
                Some(
                    "未找到便携 Python 环境（bin/python/python.exe 或 lib/），请运行 setup.bat"
                        .to_string(),
                )
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

    /// Start the ASR service. Dispatches to native or docker implementation
    /// based on `config.launch_mode`.
    pub async fn start_service(
        self: &Arc<Self>,
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

        // Clear previous state
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
        if let Ok(mut hi) = self.health_info.lock() {
            *hi = None;
        }

        match &config.launch_mode {
            ASRLaunchMode::Native { base_dir } => {
                let base_dir = base_dir.clone();
                self.spawn_native(&base_dir, &config, app_handle).await?;
            }
            ASRLaunchMode::Docker { .. } => {
                self.spawn_docker(&config, app_handle).await?;
            }
        }

        // Record the active launch mode
        if let Ok(mut guard) = self.current_mode.lock() {
            *guard = Some(config.launch_mode.clone());
        }

        Ok(())
    }

    /// Start the native backend by invoking Python directly (no batch/shell scripts).
    async fn spawn_native(
        self: &Arc<Self>,
        base_dir: &Path,
        config: &ASRStartConfig,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        // Validate path
        let validation = Self::validate_path(base_dir);
        if !validation.valid {
            return Err(format!(
                "ASR 服务路径无效：{}",
                validation.setup_hint.unwrap_or_default()
            ));
        }

        let asr_dir = base_dir.join("asr-service");
        let main_py = asr_dir.join("app").join("main.py");
        if !main_py.exists() {
            return Err(format!("未找到入口文件：{}", main_py.display()));
        }

        // Resolve Python executable
        #[cfg(target_os = "windows")]
        let python_exe = asr_dir.join("bin").join("python").join("python.exe");

        #[cfg(not(target_os = "windows"))]
        let python_exe = asr_dir.join("venv").join("bin").join("python3");

        if !python_exe.exists() {
            return Err(format!("未找到 Python：{}", python_exe.display()));
        }

        // Build args: -m app.main --host ... --port ... ...
        let mut args: Vec<String> = vec!["-m".to_string(), "app.main".to_string()];
        args.extend(build_script_args(config));

        tracing::info!(
            "[ASR] Starting ASR service (native): cwd={} cmd={} {}",
            asr_dir.display(),
            python_exe.display(),
            args.join(" ")
        );

        // Set PYTHONPATH to asr_dir so that `from app.xxx import ...` works.
        // Also add bin/python to PATH on Windows for DLL resolution (matches start.bat).
        let asr_dir_str = asr_dir.to_string_lossy().to_string();

        #[cfg(target_os = "windows")]
        let mut child = {
            let bin_dir = asr_dir.join("bin");
            let bin_python_dir = bin_dir.join("python");
            let existing_path = std::env::var("PATH").unwrap_or_default();
            let new_path = format!(
                "{};{};{}",
                bin_dir.to_string_lossy(),
                bin_python_dir.to_string_lossy(),
                existing_path
            );
            tracing::info!(
                "[ASR] Windows env: PYTHONPATH={}, PATH prepend: {} ; {}",
                asr_dir_str,
                bin_dir.display(),
                bin_python_dir.display()
            );
            tokio::process::Command::new(&python_exe)
                .args(&args)
                .current_dir(&asr_dir)
                .env("PYTHONPATH", &asr_dir_str)
                .env("PATH", &new_path)
                // Force UTF-8 mode on Windows (equivalent to chcp 65001 in start.bat)
                .env("PYTHONUTF8", "1")
                .env("PYTHONIOENCODING", "utf-8")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("启动 ASR 服务失败：{}", e))?
        };

        #[cfg(not(target_os = "windows"))]
        let mut child = {
            let mut cmd = tokio::process::Command::new(&python_exe);
            cmd.args(&args)
                .current_dir(&asr_dir)
                .env("PYTHONPATH", &asr_dir_str)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .kill_on_drop(true);
            // Create a new process group so we can kill the entire tree
            unsafe {
                cmd.pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                });
            }
            cmd.spawn()
                .map_err(|e| format!("启动 ASR 服务失败：{}", e))?
        };

        // Take stdout and stderr handles
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Store PID separately so we can kill the process tree even after
        // the Child handle is consumed by try_wait() in native_child_running().
        let pid = child.id();
        tracing::info!("[ASR] Python process spawned, PID: {:?}", pid);
        if let Ok(mut guard) = self.child_pid.lock() {
            *guard = pid;
        }

        // Store child process
        if let Ok(mut guard) = self.child.lock() {
            *guard = Some(child);
        }

        self.set_status(ASRServiceStatus::Starting);
        self.emit_status(&app_handle);

        // Spawn stdout log reader
        if let Some(stdout) = stdout {
            let mgr = Arc::clone(self);
            let app_h = app_handle.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::info!("[ASR stdout] {}", line);
                    mgr.push_log(&line, &app_h);
                }
                tracing::info!("[ASR] stdout reader finished");
            });
        }

        // Spawn stderr log reader
        if let Some(stderr) = stderr {
            let mgr = Arc::clone(self);
            let app_h = app_handle.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("[ASR stderr] {}", line);
                    mgr.push_log(&line, &app_h);
                }
                tracing::info!("[ASR] stderr reader finished");
            });
        }

        // Spawn health check loop (checks native child liveness via try_wait)
        let health_url = format!("http://{}:{}/v1/health", config.host, config.port);
        let mgr = Arc::clone(self);
        let app_h = app_handle.clone();
        tokio::spawn(async move {
            mgr.run_native_health_loop(health_url, app_h).await;
        });

        Ok(())
    }

    /// Start a docker-managed backend
    async fn spawn_docker(
        self: &Arc<Self>,
        config: &ASRStartConfig,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        let (image, data_dir, use_gpu, force_platform) = match &config.launch_mode {
            ASRLaunchMode::Docker {
                image,
                data_dir,
                use_gpu,
                force_platform,
            } => (
                image.clone(),
                data_dir.clone(),
                *use_gpu,
                force_platform.clone(),
            ),
            _ => return Err("launch_mode 不是 Docker".to_string()),
        };

        // Pre-check: container conflict
        if let Some(state) = docker::container_state(docker::CONTAINER_NAME).await {
            tracing::warn!(
                "Docker container {} already exists (state: {})",
                docker::CONTAINER_NAME,
                state
            );
            return Err(ERR_CONTAINER_CONFLICT.to_string());
        }

        // Pre-check: image pulled
        if !docker::check_image_pulled(&image).await {
            return Err(format!("镜像 {} 尚未拉取到本地，请先 pull", image));
        }

        // Prepare mount: {data_dir}/models
        let models_dir = data_dir.join("models");
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            return Err(format!("创建模型目录失败：{}", e));
        }

        // Build docker run args
        let mut run_args: Vec<String> = vec![
            "run".to_string(),
            "--rm".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            docker::CONTAINER_NAME.to_string(),
            "-p".to_string(),
            format!("127.0.0.1:{}:8765", config.port),
            "-v".to_string(),
            format!("{}:/app/models", models_dir.to_string_lossy()),
        ];
        if let Some(plat) = force_platform.as_ref() {
            run_args.push("--platform".to_string());
            run_args.push(plat.clone());
        }
        if use_gpu {
            run_args.push("--gpus".to_string());
            run_args.push("all".to_string());
        }
        run_args.push(image.clone());

        // Container-internal service args: host must be 0.0.0.0, port fixed at 8765
        let container_config = ASRStartConfig {
            launch_mode: ASRLaunchMode::Docker {
                image: image.clone(),
                data_dir: data_dir.clone(),
                use_gpu,
                force_platform: force_platform.clone(),
            },
            port: 8765,
            device: config.device.clone(),
            model_size: config.model_size.clone(),
            enable_align: config.enable_align,
            enable_punc: config.enable_punc,
            model_source: config.model_source.clone(),
            max_segment: config.max_segment,
            host: "0.0.0.0".to_string(),
        };
        for a in build_script_args(&container_config) {
            run_args.push(a);
        }

        tracing::info!(
            "Starting ASR service (docker): docker {}",
            run_args.join(" ")
        );

        let out = tokio::process::Command::new("docker")
            .args(&run_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("执行 docker run 失败：{}", e))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(format!("启动 Docker 容器失败：{}", err));
        }

        let container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        tracing::info!("Docker container started: {}", container_id);
        self.push_log(
            &format!("[docker] container started: {}", container_id),
            &app_handle,
        );

        self.set_status(ASRServiceStatus::Starting);
        self.emit_status(&app_handle);

        // Stream container logs in background
        {
            let mgr = Arc::clone(self);
            let app_h = app_handle.clone();
            tokio::spawn(async move {
                mgr.stream_docker_logs(app_h).await;
            });
        }

        // Health check + container liveness loop
        let health_url = format!("http://127.0.0.1:{}/v1/health", config.port);
        let mgr = Arc::clone(self);
        let app_h = app_handle.clone();
        tokio::spawn(async move {
            mgr.run_docker_health_loop(health_url, app_h).await;
        });

        Ok(())
    }

    /// Native-mode health polling loop
    async fn run_native_health_loop(self: Arc<Self>, health_url: String, app_h: AppHandle) {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        for var in &[
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            if let Ok(val) = std::env::var(var) {
                tracing::warn!("[ASR] Env proxy detected: {}={}", var, val);
            }
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap_or_default();

        let mut attempt: u32 = 0;
        let mut slow_mode = false;
        let mut slow_mode_attempts: u32 = 0;

        loop {
            if matches!(
                self.status(),
                ASRServiceStatus::Stopping | ASRServiceStatus::Stopped
            ) {
                return;
            }

            // On Windows, cmd.exe may exit while python.exe is still running
            // (e.g. start.bat uses `start` to launch python in a new process).
            // Don't hard-fail here — let the health check be authoritative.
            let child_exited = !self.native_child_running();
            if child_exited {
                tracing::info!(
                    "[ASR] Launcher process exited during startup (attempt {}/{}), continuing health checks...",
                    attempt + 1,
                    MAX_HEALTH_CHECK_ATTEMPTS,
                );
            }

            if self.probe_health(&client, &health_url, attempt).await {
                self.monitor_native_process(app_h, client, health_url).await;
                return;
            }

            // If the launcher has exited, fail faster to avoid unnecessary waits
            if child_exited && attempt >= 5 {
                let msg = "ASR 服务进程异常退出，请查看日志了解原因".to_string();
                tracing::warn!("{}", msg);
                self.set_status(ASRServiceStatus::Error { message: msg });
                self.emit_status(&app_h);
                return;
            }

            attempt += 1;

            // Transition to slow polling after initial fast attempts
            if !slow_mode && attempt >= MAX_HEALTH_CHECK_ATTEMPTS {
                slow_mode = true;
                let msg = "ASR 服务启动时间较长，可能正在下载模型，请查看日志了解进度。您可以继续等待或手动停止服务。".to_string();
                tracing::warn!("{}", msg);
                self.push_log(&msg, &app_h);
                let _ = app_h.emit("asr-service-slow-start", &msg);
            }

            // Abort after maximum slow-mode wait (30 minutes)
            if slow_mode {
                slow_mode_attempts += 1;
                if slow_mode_attempts >= MAX_SLOW_MODE_ATTEMPTS {
                    let msg = "ASR 服务在 30 分钟内仍未就绪，已自动停止。请检查日志排查问题。"
                        .to_string();
                    tracing::error!("{}", msg);
                    self.set_status(ASRServiceStatus::Error { message: msg });
                    self.emit_status(&app_h);
                    return;
                }
            }

            let interval_secs = if slow_mode {
                15
            } else {
                HEALTH_CHECK_INTERVAL_SECS
            };
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    }

    /// Docker-mode health polling loop
    async fn run_docker_health_loop(self: Arc<Self>, health_url: String, app_h: AppHandle) {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap_or_default();

        let mut attempt: u32 = 0;
        let mut slow_mode = false;
        let mut slow_mode_attempts: u32 = 0;

        loop {
            if matches!(
                self.status(),
                ASRServiceStatus::Stopping | ASRServiceStatus::Stopped
            ) {
                return;
            }
            // Container still running?
            match docker::container_state(docker::CONTAINER_NAME).await {
                Some(s) if s == "running" => {}
                other => {
                    let msg = format!(
                        "Docker 容器未在运行（状态: {}），请查看日志",
                        other.unwrap_or_else(|| "not found".to_string())
                    );
                    tracing::warn!("{}", msg);
                    self.set_status(ASRServiceStatus::Error { message: msg });
                    self.emit_status(&app_h);
                    return;
                }
            }

            if self.probe_health(&client, &health_url, attempt).await {
                self.monitor_docker_container(app_h).await;
                return;
            }

            attempt += 1;

            // Transition to slow polling after initial fast attempts
            if !slow_mode && attempt >= MAX_HEALTH_CHECK_ATTEMPTS {
                slow_mode = true;
                let msg = "ASR 服务启动时间较长，可能正在下载模型，请查看日志了解进度。您可以继续等待或手动停止服务。".to_string();
                tracing::warn!("{}", msg);
                self.push_log(&msg, &app_h);
                let _ = app_h.emit("asr-service-slow-start", &msg);
            }

            // Abort after maximum slow-mode wait (30 minutes)
            if slow_mode {
                slow_mode_attempts += 1;
                if slow_mode_attempts >= MAX_SLOW_MODE_ATTEMPTS {
                    let msg = "ASR 服务在 30 分钟内仍未就绪，已自动停止。请检查日志排查问题。"
                        .to_string();
                    tracing::error!("{}", msg);
                    self.set_status(ASRServiceStatus::Error { message: msg });
                    self.emit_status(&app_h);
                    return;
                }
            }

            let interval_secs = if slow_mode {
                15
            } else {
                HEALTH_CHECK_INTERVAL_SECS
            };
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    }

    /// Perform a single /v1/health probe and update state on success.
    /// Returns true when the service is healthy.
    async fn probe_health(&self, client: &reqwest::Client, health_url: &str, attempt: u32) -> bool {
        match client.get(health_url).send().await {
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
                    if let Ok(mut hi) = self.health_info.lock() {
                        *hi = Some(health);
                    }
                    self.set_status(ASRServiceStatus::Running);
                    return true;
                }
                false
            }
            Ok(resp) => {
                let status = resp.status();
                let headers: Vec<String> = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("?")))
                    .collect();
                let body = resp.text().await.unwrap_or_default();
                tracing::info!(
                    "ASR health check attempt {}/{}: HTTP {} headers={:?} body={}",
                    attempt + 1,
                    MAX_HEALTH_CHECK_ATTEMPTS,
                    status,
                    headers,
                    body,
                );
                false
            }
            Err(e) => {
                tracing::warn!(
                    "ASR health check attempt {}/{}: {} (url: {})",
                    attempt + 1,
                    MAX_HEALTH_CHECK_ATTEMPTS,
                    e,
                    health_url,
                );
                false
            }
        }
    }

    /// Monitor native child process; if it exits, verify service health
    /// before reporting an error (on Windows, cmd.exe may exit while the
    /// actual python service is still running).
    async fn monitor_native_process(
        self: Arc<Self>,
        app_handle: AppHandle,
        health_client: reqwest::Client,
        health_url: String,
    ) {
        // Emit Running once before entering monitor loop
        self.emit_status(&app_handle);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            let current = self.status();
            if current == ASRServiceStatus::Stopping || current == ASRServiceStatus::Stopped {
                return;
            }
            if !self.native_child_running() {
                // Launcher process exited — verify service is still healthy
                tracing::info!("[ASR] Launcher process exited, verifying service health...");
                match health_client.get(&health_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!("[ASR] Service still healthy after launcher exit");
                        continue;
                    }
                    _ => {
                        let msg = "ASR 服务进程异常退出".to_string();
                        tracing::warn!("{}", msg);
                        if let Ok(mut hi) = self.health_info.lock() {
                            *hi = None;
                        }
                        self.set_status(ASRServiceStatus::Error { message: msg });
                        self.emit_status(&app_handle);
                        return;
                    }
                }
            }
        }
    }

    /// Monitor docker container; if it stops unexpectedly, set Error status
    async fn monitor_docker_container(self: Arc<Self>, app_handle: AppHandle) {
        self.emit_status(&app_handle);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            let current = self.status();
            if current == ASRServiceStatus::Stopping || current == ASRServiceStatus::Stopped {
                return;
            }
            match docker::container_state(docker::CONTAINER_NAME).await {
                Some(s) if s == "running" => continue,
                other => {
                    let msg = format!(
                        "Docker 容器已停止运行（状态: {}）",
                        other.unwrap_or_else(|| "not found".to_string())
                    );
                    tracing::warn!("{}", msg);
                    if let Ok(mut hi) = self.health_info.lock() {
                        *hi = None;
                    }
                    self.set_status(ASRServiceStatus::Error { message: msg });
                    self.emit_status(&app_handle);
                    return;
                }
            }
        }
    }

    /// Stream `docker logs -f` output to the log buffer / event bus.
    /// Exits when the container stops.
    async fn stream_docker_logs(self: Arc<Self>, app_handle: AppHandle) {
        let child = tokio::process::Command::new("docker")
            .args(["logs", "-f", docker::CONTAINER_NAME])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to attach docker logs: {}", e);
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let mgr1 = Arc::clone(&self);
        let app_h1 = app_handle.clone();
        let out_task = async move {
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    mgr1.push_log(&line, &app_h1);
                }
            }
        };

        let mgr2 = Arc::clone(&self);
        let app_h2 = app_handle.clone();
        let err_task = async move {
            if let Some(stderr) = stderr {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    mgr2.push_log(&line, &app_h2);
                }
            }
        };

        tokio::join!(out_task, err_task);
        let _ = child.wait().await;
    }

    /// Push a log line into the ring buffer and emit event
    fn push_log(&self, line: &str, app_handle: &AppHandle) {
        if let Ok(mut logs) = self.logs.lock() {
            if logs.len() >= MAX_LOG_LINES {
                logs.pop_front();
            }
            logs.push_back(line.to_string());
        }
        let _ = app_handle.emit("asr-service-log", line);
    }

    /// Stop the ASR service (native or docker)
    pub async fn stop(&self) -> Result<(), String> {
        self.set_status(ASRServiceStatus::Stopping);

        // Take a snapshot of current mode
        let mode = self.current_mode.lock().ok().and_then(|m| m.clone());

        match mode {
            Some(ASRLaunchMode::Docker { .. }) => {
                tracing::info!("Stopping ASR service (docker)...");
                // docker stop triggers --rm cleanup
                if let Err(e) = docker::stop_container(docker::CONTAINER_NAME).await {
                    tracing::warn!("docker stop warning: {}", e);
                }
                // Best-effort remove in case --rm didn't fire
                let _ = docker::remove_container(docker::CONTAINER_NAME).await;
            }
            _ => {
                // Native (or unknown) — kill child process tree
                let child = self.child.lock().ok().and_then(|mut g| g.take());

                // Read the saved PID (may exist even if Child handle was already consumed)
                let saved_pid = self.child_pid.lock().ok().and_then(|mut g| g.take());

                if let Some(mut child) = child {
                    tracing::info!("Stopping ASR service (native, killing child tree)...");

                    #[cfg(not(target_os = "windows"))]
                    {
                        if let Some(pid) = child.id() {
                            unsafe {
                                libc::kill(-(pid as i32), libc::SIGTERM);
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            if child.try_wait().ok().flatten().is_none() {
                                unsafe {
                                    libc::kill(-(pid as i32), libc::SIGKILL);
                                }
                            }
                        }
                    }

                    #[cfg(target_os = "windows")]
                    {
                        if let Some(pid) = child.id() {
                            kill_process_tree(pid).await;
                        }
                    }

                    let _ = child.wait().await;
                } else if let Some(pid) = saved_pid {
                    // Child handle was consumed (e.g. by native_child_running detecting
                    // cmd.exe exit), but the real ASR process may still be alive.
                    tracing::info!(
                        "No child handle but saved PID {} exists, killing process tree...",
                        pid
                    );
                    #[cfg(not(target_os = "windows"))]
                    {
                        unsafe {
                            libc::kill(pid as i32, libc::SIGTERM);
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        kill_process_tree(pid).await;
                    }
                } else {
                    tracing::info!("No child process found, updating status only");
                }
            }
        }

        if let Ok(mut hi) = self.health_info.lock() {
            *hi = None;
        }
        if let Ok(mut m) = self.current_mode.lock() {
            *m = None;
        }
        if let Ok(mut p) = self.child_pid.lock() {
            *p = None;
        }

        self.set_status(ASRServiceStatus::Stopped);
        Ok(())
    }

    /// Force-remove any leftover docker container with the fixed name.
    /// Called when the user confirms the "container conflict" dialog.
    pub async fn force_remove_docker_container() -> Result<(), String> {
        docker::remove_container(docker::CONTAINER_NAME).await
    }

    /// Open an external terminal to run the setup script (interactive, needs user input).
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
        let launch_kind = self.current_mode.lock().ok().and_then(|m| {
            m.as_ref().map(|mode| match mode {
                ASRLaunchMode::Native { .. } => "native",
                ASRLaunchMode::Docker { .. } => "docker",
            })
        });
        ASRServiceStatusInfo {
            status: self.status(),
            health_info: self.health_info(),
            launch_kind,
        }
    }

    /// Whether the backing backend is still alive.
    /// For native: queries the child process.
    /// For docker: relies on the current service status (monitor task updates it).
    pub fn is_running(&self) -> bool {
        let mode = self.current_mode.lock().ok().and_then(|m| m.clone());
        match mode {
            Some(ASRLaunchMode::Docker { .. }) => matches!(
                self.status(),
                ASRServiceStatus::Running | ASRServiceStatus::Starting
            ),
            _ => self.native_child_running(),
        }
    }

    /// Check if the native child process is still running.
    /// On exit, the Child handle is consumed but the saved PID is preserved
    /// so stop() can still kill the process tree.
    fn native_child_running(&self) -> bool {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(None) => return true,
                    Ok(Some(status)) => {
                        tracing::warn!(
                            "[ASR] Python process exited — code: {:?}, success: {}",
                            status.code(),
                            status.success()
                        );
                        // Python process exited — consume the handle but keep child_pid
                        // so stop() can still kill the process tree if needed.
                        *guard = None;
                        return false;
                    }
                    Err(e) => {
                        tracing::warn!("[ASR] Failed to query process status: {}", e);
                        return false;
                    }
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
        // On Windows, kill the entire process tree (not just cmd.exe).
        // On Unix, start_kill() is sufficient because kill_on_drop was set and
        // process groups are used.
        #[cfg(target_os = "windows")]
        {
            if let Ok(guard) = self.child_pid.lock() {
                if let Some(pid) = *guard {
                    // Synchronous taskkill — Drop cannot be async
                    let _ = std::process::Command::new("taskkill")
                        .args(["/T", "/F", "/PID", &pid.to_string()])
                        .output();
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.start_kill();
            }
        }
    }
}

// ==================== Helpers ====================

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

/// Build command-line arguments shared by native script and docker entrypoint
fn build_script_args(config: &ASRStartConfig) -> Vec<String> {
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

/// Kill an entire process tree on Windows using `taskkill /T /F /PID`.
/// `/T` recursively terminates all descendant processes.
/// `/F` forces termination.
#[cfg(target_os = "windows")]
async fn kill_process_tree(pid: u32) {
    tracing::info!("Killing process tree with taskkill /T /F /PID {}", pid);
    let result = tokio::process::Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    match result {
        Ok(out) if out.status.success() => {
            tracing::info!("taskkill succeeded for PID {}", pid);
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            // "not found" is fine — process already exited
            if stderr.contains("not found") || stderr.contains("not running") {
                tracing::info!("Process {} already exited", pid);
            } else {
                tracing::warn!("taskkill for PID {} returned error: {}", pid, stderr.trim());
            }
        }
        Err(e) => {
            tracing::warn!("Failed to run taskkill for PID {}: {}", pid, e);
        }
    }
}
