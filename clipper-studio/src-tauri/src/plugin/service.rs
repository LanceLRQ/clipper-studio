use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use super::manifest::PlatformCommand;
use super::transport::{HttpTransport, PluginTransport};

/// Maximum log lines retained per service
const MAX_LOG_LINES: usize = 500;
/// Auto-restart limit: max restarts within the window before giving up
const MAX_RESTARTS: u32 = 3;
/// Auto-restart window in seconds
const RESTART_WINDOW_SECS: u64 = 300;

/// Tracks auto-restart state to prevent restart storms
struct RestartState {
    count: u32,
    window_start: Option<std::time::Instant>,
}

/// Manages a single long-running service plugin process
pub struct ServiceManager {
    plugin_id: String,
    child: Mutex<Option<Child>>,
    logs: Mutex<VecDeque<String>>,
    port: u16,
    health_endpoint: String,
    log_tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// Stored startup config for auto-restart
    startup_config: Mutex<Option<(super::manifest::PlatformCommand, std::path::PathBuf)>>,
    restart_state: Mutex<RestartState>,
}

impl ServiceManager {
    pub fn new(plugin_id: &str, port: u16, health_endpoint: &str) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            child: Mutex::new(None),
            logs: Mutex::new(VecDeque::new()),
            port,
            health_endpoint: health_endpoint.to_string(),
            log_tasks: Mutex::new(Vec::new()),
            startup_config: Mutex::new(None),
            restart_state: Mutex::new(RestartState {
                count: 0,
                window_start: None,
            }),
        }
    }

    /// Start the service process
    pub async fn start(&self, startup: &PlatformCommand, working_dir: &Path) -> Result<(), String> {
        // Store startup config for potential auto-restart
        if let Ok(mut guard) = self.startup_config.lock() {
            *guard = Some((startup.clone(), working_dir.to_path_buf()));
        }

        // Check if already running
        if self.is_running() {
            return Ok(());
        }

        let cmd_str = startup
            .current()
            .ok_or("No startup command for current platform")?;

        tracing::info!("[{}] Starting service: {}", self.plugin_id, cmd_str);

        // Parse command string (support "bash start.sh" style)
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Empty startup command".to_string());
        }

        let mut cmd = Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }

        let mut child = cmd
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to start service: {}", e))?;

        // Capture stdout/stderr logs in background, save handles for clean shutdown
        let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        if let Some(stdout) = child.stdout.take() {
            let plugin_id = self.plugin_id.clone();
            handles.push(tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!("[{}] {}", plugin_id, line);
                }
            }));
        }

        if let Some(stderr) = child.stderr.take() {
            let plugin_id = self.plugin_id.clone();
            handles.push(tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("[{}] stderr: {}", plugin_id, line);
                }
            }));
        }

        if let Ok(mut guard) = self.log_tasks.lock() {
            *guard = handles;
        }

        {
            if let Ok(mut guard) = self.child.lock() {
                *guard = Some(child);
            }
        }

        // Wait briefly for service to initialize, then health check
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Retry health check up to 5 times
        for i in 0..5 {
            if self.health_check().await {
                tracing::info!("[{}] Service is healthy", self.plugin_id);
                // Reset restart counter on successful start
                if let Ok(mut rs) = self.restart_state.lock() {
                    rs.count = 0;
                    rs.window_start = None;
                }
                return Ok(());
            }
            if i < 4 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }

        tracing::warn!(
            "[{}] Service started but health check failed (may still be initializing)",
            self.plugin_id
        );
        Ok(())
    }

    /// Stop the service process
    pub async fn stop(&self) -> Result<(), String> {
        // Clear startup config and restart state to prevent auto-restart after explicit stop
        if let Ok(mut guard) = self.startup_config.lock() {
            *guard = None;
        }
        if let Ok(mut rs) = self.restart_state.lock() {
            rs.count = MAX_RESTARTS; // Prevent further restarts
        }

        // Abort log capture tasks
        if let Ok(mut guard) = self.log_tasks.lock() {
            for handle in guard.drain(..) {
                handle.abort();
            }
        }

        // Extract child from lock before awaiting (MutexGuard is not Send)
        let child = self.child.lock().ok().and_then(|mut g| g.take());
        if let Some(mut child) = child {
            tracing::info!("[{}] Stopping service", self.plugin_id);
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(())
    }

    /// Check if the service process is still running
    pub fn is_running(&self) -> bool {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                // try_wait returns Ok(Some(status)) if exited, Ok(None) if still running
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

    /// Health check via HTTP endpoint
    pub async fn health_check(&self) -> bool {
        let transport = HttpTransport::new(
            &format!("http://127.0.0.1:{}", self.port),
            Some(&self.health_endpoint),
        );
        let result: Result<bool, String> = transport.health().await;
        result.unwrap_or(false)
    }

    /// Check if the service has crashed and auto-restart if within limits.
    /// Returns true if service is running (or was successfully restarted).
    pub async fn check_and_restart(&self) -> bool {
        if self.is_running() {
            return true;
        }

        // Service has crashed — check restart limits
        let can_restart = {
            if let Ok(mut rs) = self.restart_state.lock() {
                let now = std::time::Instant::now();
                // Reset window if expired
                if let Some(start) = rs.window_start {
                    if now.duration_since(start).as_secs() > RESTART_WINDOW_SECS {
                        rs.count = 0;
                        rs.window_start = None;
                    }
                }
                if rs.count < MAX_RESTARTS {
                    rs.count += 1;
                    if rs.window_start.is_none() {
                        rs.window_start = Some(now);
                    }
                    true
                } else {
                    tracing::error!(
                        "[{}] Restart limit exceeded ({} in {}s), giving up",
                        self.plugin_id, rs.count, RESTART_WINDOW_SECS
                    );
                    false
                }
            } else {
                false
            }
        };

        if !can_restart {
            return false;
        }

        // Retrieve stored startup config
        let config = self
            .startup_config
            .lock()
            .ok()
            .and_then(|mut g| g.take());

        if let Some((startup, working_dir)) = config {
            tracing::warn!(
                "[{}] Process crashed, auto-restarting (attempt this window)",
                self.plugin_id
            );
            match self.start(&startup, &working_dir).await {
                Ok(()) => {
                    tracing::info!("[{}] Auto-restart successful", self.plugin_id);
                    // Restore config for future restarts
                    if let Ok(mut guard) = self.startup_config.lock() {
                        *guard = Some((startup, working_dir));
                    }
                    true
                }
                Err(e) => {
                    tracing::error!("[{}] Auto-restart failed: {}", self.plugin_id, e);
                    false
                }
            }
        } else {
            tracing::warn!("[{}] No startup config stored, cannot auto-restart", self.plugin_id);
            false
        }
    }

    /// Get the service base URL
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Add a log line
    pub fn add_log(&self, line: &str) {
        if let Ok(mut logs) = self.logs.lock() {
            if logs.len() >= MAX_LOG_LINES {
                logs.pop_front();
            }
            logs.push_back(line.to_string());
        }
    }

    /// Get recent log lines
    pub fn get_logs(&self) -> Vec<String> {
        self.logs
            .lock()
            .map(|logs| logs.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl Drop for ServiceManager {
    fn drop(&mut self) {
        // Attempt synchronous kill on drop
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.start_kill();
            }
        }
    }
}
