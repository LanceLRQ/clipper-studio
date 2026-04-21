use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

/// Fixed container name used across the app
pub const CONTAINER_NAME: &str = "qwen3-asr-service";

/// Result of probing the host for Docker CLI + daemon availability
#[derive(Debug, Clone, Serialize)]
pub struct DockerCapability {
    /// `docker --version` exits successfully
    pub installed: bool,
    /// `docker info` exits successfully (daemon reachable)
    pub daemon_running: bool,
    /// "amd64" | "arm64" | "unknown"
    pub host_arch: String,
    /// "macos" | "windows" | "linux"
    pub host_platform: String,
    /// `docker --version` short string
    pub version: Option<String>,
    /// Human-readable hint for UI when docker is not usable
    pub hint: Option<String>,
}

/// Detect Docker CLI and daemon status
pub async fn detect_docker() -> DockerCapability {
    let host_platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
    .to_string();

    let host_arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "amd64"
    } else {
        "unknown"
    }
    .to_string();

    // docker --version
    let version_out = Command::new("docker")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    let (installed, version) = match version_out {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (true, if s.is_empty() { None } else { Some(s) })
        }
        _ => (false, None),
    };

    if !installed {
        return DockerCapability {
            installed: false,
            daemon_running: false,
            host_arch,
            host_platform,
            version: None,
            hint: Some("未检测到 Docker，请先安装 Docker Desktop 或 Docker Engine".to_string()),
        };
    }

    // docker info
    let info_out = Command::new("docker")
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    let daemon_running = matches!(info_out, Ok(o) if o.status.success());

    let hint = if !daemon_running {
        Some("Docker 已安装但守护进程未运行，请启动 Docker Desktop 后刷新".to_string())
    } else {
        None
    };

    DockerCapability {
        installed,
        daemon_running,
        host_arch,
        host_platform,
        version,
        hint,
    }
}

/// Check whether an image is pulled locally via `docker image inspect`
pub async fn check_image_pulled(image: &str) -> bool {
    let out = Command::new("docker")
        .args(["image", "inspect", image])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;
    matches!(out, Ok(o) if o.status.success())
}

/// Query container state via `docker inspect -f {{.State.Status}}`.
/// Returns Some("running" | "exited" | "created" | ...) if exists, None otherwise.
pub async fn container_state(name: &str) -> Option<String> {
    let out = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Status}}", name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Force remove a container (ignores errors if it does not exist)
pub async fn remove_container(name: &str) -> Result<(), String> {
    let out = Command::new("docker")
        .args(["rm", "-f", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("执行 docker rm 失败：{}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // If the container doesn't exist, treat as success
        if err.contains("No such container") || err.contains("is not running") {
            return Ok(());
        }
        return Err(format!("docker rm 失败：{}", err));
    }
    Ok(())
}

/// Stop a container (does not remove; `--rm` on run takes care of removal)
pub async fn stop_container(name: &str) -> Result<(), String> {
    let out = Command::new("docker")
        .args(["stop", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("执行 docker stop 失败：{}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.contains("No such container") || err.contains("is not running") {
            return Ok(());
        }
        return Err(format!("docker stop 失败：{}", err));
    }
    Ok(())
}

/// Open the system terminal and run `docker pull <image>` so the user can
/// see native progress bars. Does NOT manage the process lifecycle.
pub fn open_pull_terminal(image: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // cmd /C start "Docker Pull" cmd /K docker pull <image>
        std::process::Command::new("cmd")
            .args([
                "/C",
                "start",
                "Docker Pull",
                "cmd",
                "/K",
                &format!("docker pull {}", image),
            ])
            .spawn()
            .map_err(|e| format!("启动终端失败：{}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        let osa_script = format!(
            "tell application \"Terminal\"\nactivate\ndo script \"docker pull {}\"\nend tell",
            image
        );
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(&osa_script)
            .spawn()
            .map_err(|e| format!("启动终端失败：{}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let cmd_str = format!("docker pull {} ; exec bash", image);
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
