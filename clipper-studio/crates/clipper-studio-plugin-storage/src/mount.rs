//! Platform-specific SMB/CIFS mount implementations
//!
//! - macOS: `mount_smbfs //user:pass@server/share /mount/point`
//! - Linux: `mount -t cifs //server/share /mount/point -o username=xxx,password=xxx`
//! - Windows: `net use X: \\server\share /user:xxx pass`

use serde::{Deserialize, Serialize};

/// Parameters for mounting a network share
#[derive(Debug, Clone)]
pub struct MountParams {
    pub server: String,
    pub share: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Explicit local mount point; auto-generated if None
    pub mount_point: Option<String>,
}

/// Information about an active mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    pub server: String,
    pub share: String,
    pub mount_point: String,
}

/// Platform-specific mount operations
pub struct MountBackend;

impl MountBackend {
    /// Check if current platform is supported for SMB mounting
    pub fn is_supported() -> bool {
        cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        ))
    }

    /// Mount a SMB share and return the local mount point
    pub async fn mount(params: MountParams) -> Result<MountInfo, String> {
        #[cfg(target_os = "macos")]
        return Self::mount_macos(params).await;

        #[cfg(target_os = "linux")]
        return Self::mount_linux(params).await;

        #[cfg(target_os = "windows")]
        return Self::mount_windows(params).await;

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        Err("Unsupported platform".to_string())
    }

    /// Unmount a previously mounted share
    pub async fn unmount(mount_point: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        return Self::unmount_unix(mount_point).await;

        #[cfg(target_os = "linux")]
        return Self::unmount_unix(mount_point).await;

        #[cfg(target_os = "windows")]
        return Self::unmount_windows(mount_point).await;

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        Err("Unsupported platform".to_string())
    }

    // ==================== macOS ====================

    #[cfg(target_os = "macos")]
    async fn mount_macos(params: MountParams) -> Result<MountInfo, String> {
        let mount_point = params
            .mount_point
            .clone()
            .unwrap_or_else(|| Self::auto_mount_point(&params.server, &params.share));

        // Create mount point directory
        tokio::fs::create_dir_all(&mount_point)
            .await
            .map_err(|e| format!("Failed to create mount point: {}", e))?;

        // Build SMB URL: //user:pass@server/share
        let smb_url = Self::build_smb_url(&params);

        let output = tokio::process::Command::new("mount_smbfs")
            .arg("-N") // Don't prompt for password (use URL credentials)
            .arg(&smb_url)
            .arg(&mount_point)
            .output()
            .await
            .map_err(|e| format!("Failed to execute mount_smbfs: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up empty directory on failure
            let _ = tokio::fs::remove_dir(&mount_point).await;
            return Err(format!("mount_smbfs failed: {}", stderr.trim()));
        }

        Ok(MountInfo {
            server: params.server,
            share: params.share,
            mount_point,
        })
    }

    #[cfg(target_os = "macos")]
    fn build_smb_url(params: &MountParams) -> String {
        match (&params.username, &params.password) {
            (Some(user), Some(pass)) => {
                // URL-encode special characters in password
                let encoded_pass = Self::url_encode(pass);
                format!(
                    "//{}:{}@{}/{}",
                    user, encoded_pass, params.server, params.share
                )
            }
            (Some(user), None) => {
                format!("//{}@{}/{}", user, params.server, params.share)
            }
            _ => {
                format!("//guest@{}/{}", params.server, params.share)
            }
        }
    }

    // ==================== Linux ====================

    #[cfg(target_os = "linux")]
    async fn mount_linux(params: MountParams) -> Result<MountInfo, String> {
        let mount_point = params
            .mount_point
            .clone()
            .unwrap_or_else(|| Self::auto_mount_point(&params.server, &params.share));

        // Create mount point directory
        tokio::fs::create_dir_all(&mount_point)
            .await
            .map_err(|e| format!("Failed to create mount point: {}", e))?;

        let unc_path = format!("//{}/{}", params.server, params.share);

        // Build mount options
        let mut opts = Vec::new();
        if let Some(ref user) = params.username {
            opts.push(format!("username={}", user));
        } else {
            opts.push("guest".to_string());
        }
        if let Some(ref pass) = params.password {
            opts.push(format!("password={}", pass));
        }
        // uid/gid to current user for permission
        opts.push(format!("uid={}", unsafe { libc::getuid() }));
        opts.push(format!("gid={}", unsafe { libc::getgid() }));

        let opts_str = opts.join(",");

        let output = tokio::process::Command::new("mount")
            .arg("-t")
            .arg("cifs")
            .arg(&unc_path)
            .arg(&mount_point)
            .arg("-o")
            .arg(&opts_str)
            .output()
            .await
            .map_err(|e| format!("Failed to execute mount: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tokio::fs::remove_dir(&mount_point).await;
            return Err(format!(
                "mount.cifs failed: {}（提示：Linux 下可能需要 sudo 权限或 /etc/fstab 配置）",
                stderr.trim()
            ));
        }

        Ok(MountInfo {
            server: params.server,
            share: params.share,
            mount_point,
        })
    }

    // ==================== Windows ====================

    #[cfg(target_os = "windows")]
    async fn mount_windows(params: MountParams) -> Result<MountInfo, String> {
        // On Windows, use `net use` to map a drive letter or UNC path
        let unc_path = format!("\\\\{}\\{}", params.server, params.share);

        // If mount_point is specified, use it as drive letter (e.g. "Z:")
        // Otherwise, use * to auto-assign
        let drive = params
            .mount_point
            .clone()
            .unwrap_or_else(|| "*".to_string());

        let mut cmd = tokio::process::Command::new("net");
        cmd.arg("use").arg(&drive).arg(&unc_path);

        if let Some(ref user) = params.username {
            cmd.arg(format!("/user:{}", user));
        }
        if let Some(ref pass) = params.password {
            cmd.arg(pass);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to execute net use: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Error 1219: multiple connections with different credentials
            if stderr.contains("1219") {
                return Err(format!(
                    "该服务器已存在使用其他凭据的连接，请先在命令行执行 \
                     `net use \\\\{}\\{} /delete` 断开后重试",
                    params.server, params.share
                ));
            }
            return Err(format!("net use failed: {}", stderr.trim()));
        }

        // Parse the assigned drive letter from stdout if we used "*"
        let mount_point = if drive == "*" {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // net use output: "Drive Z: is now connected to \\server\share."
            Self::parse_net_use_drive(&stdout).unwrap_or_else(|| unc_path.clone())
        } else {
            drive
        };

        Ok(MountInfo {
            server: params.server,
            share: params.share,
            mount_point,
        })
    }

    #[cfg(target_os = "windows")]
    fn parse_net_use_drive(output: &str) -> Option<String> {
        // Look for pattern like "Drive X:" or "驱动器 X:"
        for word in output.split_whitespace() {
            if word.len() == 2
                && word.ends_with(':')
                && word
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_ascii_alphabetic())
            {
                return Some(word.to_uppercase());
            }
        }
        None
    }

    // ==================== Unmount ====================

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    async fn unmount_unix(mount_point: &str) -> Result<(), String> {
        let output = tokio::process::Command::new("umount")
            .arg(mount_point)
            .output()
            .await
            .map_err(|e| format!("Failed to execute umount: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Try force unmount on macOS
            #[cfg(target_os = "macos")]
            {
                let force = tokio::process::Command::new("diskutil")
                    .arg("unmount")
                    .arg("force")
                    .arg(mount_point)
                    .output()
                    .await;
                if let Ok(f) = force {
                    if f.status.success() {
                        return Ok(());
                    }
                }
            }
            return Err(format!("umount failed: {}", stderr.trim()));
        }

        // Try to clean up empty mount point directory
        let _ = tokio::fs::remove_dir(mount_point).await;

        Ok(())
    }

    #[cfg(target_os = "windows")]
    async fn unmount_windows(mount_point: &str) -> Result<(), String> {
        let output = tokio::process::Command::new("net")
            .arg("use")
            .arg(mount_point)
            .arg("/delete")
            .arg("/y")
            .output()
            .await
            .map_err(|e| format!("Failed to execute net use /delete: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("net use /delete failed: {}", stderr.trim()));
        }

        Ok(())
    }

    // ==================== Detection ====================

    /// Check if a given path is currently a mount point
    pub async fn is_mount_point(path: &str) -> bool {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            // Use `mount` command and grep for the path
            let output = tokio::process::Command::new("mount").output().await;
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    // mount output: "//server/share on /mount/point ..."
                    stdout.lines().any(|line| {
                        line.split(" on ")
                            .nth(1)
                            .map(|rest| rest.split_whitespace().next().unwrap_or("") == path)
                            .unwrap_or(false)
                    })
                }
                Err(_) => false,
            }
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, check if the drive letter / UNC path is accessible
            let output = tokio::process::Command::new("net")
                .arg("use")
                .output()
                .await;
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let path_upper = path.to_uppercase();
                    stdout
                        .lines()
                        .any(|line| line.to_uppercase().contains(&path_upper))
                }
                Err(_) => false,
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        false
    }

    // ==================== Helpers ====================

    /// Generate an automatic mount point path
    #[allow(dead_code)]
    fn auto_mount_point(_server: &str, _share: &str) -> String {
        #[cfg(target_os = "windows")]
        {
            // Windows: let net use auto-assign
            "*".to_string()
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Unix: /tmp/clipper-mounts/{server}_{share}
            let dir_name = format!("{}_{}", _server.replace('.', "-"), _share);
            let base = std::env::temp_dir().join("clipper-mounts").join(dir_name);
            base.to_string_lossy().to_string()
        }
    }

    /// URL-encode special characters for SMB URL (macOS)
    #[cfg(target_os = "macos")]
    fn url_encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}
