use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::registry::{ArchiveType, ExtractMapping};

// ==================== Progress Events ====================

#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub dep_id: String,
    pub phase: String,
    pub progress: f64,
    pub message: String,
}

/// Emit install progress event (public for use by DependencyManager)
pub fn emit_progress_static(app_handle: &AppHandle, dep_id: &str, phase: &str, progress: f64, message: &str) {
    emit_progress(app_handle, dep_id, phase, progress, message);
}

fn emit_progress(app_handle: &AppHandle, dep_id: &str, phase: &str, progress: f64, message: &str) {
    let _ = app_handle.emit(
        "dep:install-progress",
        InstallProgress {
            dep_id: dep_id.to_string(),
            phase: phase.to_string(),
            progress,
            message: message.to_string(),
        },
    );
}

// ==================== Download ====================

/// Download a file from URL to a local path, with progress reporting.
/// `label` is shown in the progress message (e.g. "FFmpeg (1/2)").
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    dep_id: &str,
    label: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    tracing::info!("Downloading {} -> {}", url, dest.display());
    emit_progress(
        app_handle,
        dep_id,
        "downloading",
        0.0,
        &format!("开始下载 {}...", label),
    );

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(format!("Download failed with status: {}", status));
    }

    let total_size = response.content_length();
    let total_str = total_size
        .map(|s| format_bytes(s))
        .unwrap_or_else(|| "unknown".to_string());

    // Create parent directory
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create download directory: {}", e))?;
    }

    let mut file = std::fs::File::create(dest)
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream error: {}", e))?;
        file.write_all(&chunk)
            .map_err(|e| format!("File write error: {}", e))?;

        downloaded += chunk.len() as u64;
        let progress = total_size
            .map(|total| downloaded as f64 / total as f64)
            .unwrap_or(0.0);

        let msg = format!(
            "{} — {} / {}",
            label,
            format_bytes(downloaded),
            total_str
        );
        emit_progress(app_handle, dep_id, "downloading", progress, &msg);
    }

    emit_progress(
        app_handle,
        dep_id,
        "downloading",
        1.0,
        &format!("{} 下载完成", label),
    );
    tracing::info!("Download complete: {} ({} bytes)", label, downloaded);
    Ok(())
}

// ==================== Extract ====================

/// Extract an archive to a target directory, applying extract mappings
pub fn extract_archive(
    archive_path: &Path,
    target_dir: &Path,
    archive_type: ArchiveType,
    mappings: &[ExtractMapping],
    dep_id: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    tracing::info!(
        "Extracting {} -> {}",
        archive_path.display(),
        target_dir.display()
    );
    emit_progress(app_handle, dep_id, "extracting", 0.0, "正在解压...");

    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create target directory: {}", e))?;

    match archive_type {
        ArchiveType::Zip => extract_zip(archive_path, target_dir, mappings, dep_id, app_handle),
        ArchiveType::TarGz => {
            extract_tar_gz(archive_path, target_dir, mappings, dep_id, app_handle)
        }
    }
}

fn extract_zip(
    archive_path: &Path,
    target_dir: &Path,
    mappings: &[ExtractMapping],
    dep_id: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;

    let total = archive.len();

    if mappings.is_empty() {
        // No mappings: extract everything
        for i in 0..total {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Zip entry error: {}", e))?;

            if entry.is_dir() {
                continue;
            }

            let out_path = target_dir.join(
                entry
                    .enclosed_name()
                    .ok_or_else(|| "Invalid zip entry name".to_string())?,
            );

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Create dir error: {}", e))?;
            }

            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| format!("Create file error: {}", e))?;
            io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("Extract file error: {}", e))?;

            // Set executable permission on Unix
            #[cfg(unix)]
            set_executable(&out_path);

            let progress = (i + 1) as f64 / total as f64;
            emit_progress(app_handle, dep_id, "extracting", progress, "正在解压...");
        }
    } else {
        // With mappings: only extract matched files
        let mut matched = 0;
        for i in 0..total {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Zip entry error: {}", e))?;

            if entry.is_dir() {
                continue;
            }

            let entry_name = entry.name().to_string();

            // Check if this entry matches any mapping
            for mapping in mappings {
                if glob_matches(&entry_name, mapping.archive_glob) {
                    let out_path = target_dir.join(mapping.target_name);
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("Create dir error: {}", e))?;
                    }
                    let mut out_file = std::fs::File::create(&out_path)
                        .map_err(|e| format!("Create file error: {}", e))?;
                    io::copy(&mut entry, &mut out_file)
                        .map_err(|e| format!("Extract file error: {}", e))?;

                    #[cfg(unix)]
                    set_executable(&out_path);

                    matched += 1;
                    tracing::info!("Extracted: {} -> {}", entry_name, mapping.target_name);
                    break;
                }
            }

            let progress = (i + 1) as f64 / total as f64;
            emit_progress(app_handle, dep_id, "extracting", progress, "正在解压...");
        }

        if matched == 0 {
            return Err("No files matched the extract mappings".to_string());
        }
    }

    emit_progress(app_handle, dep_id, "extracting", 1.0, "解压完成");
    Ok(())
}

fn extract_tar_gz(
    archive_path: &Path,
    target_dir: &Path,
    mappings: &[ExtractMapping],
    dep_id: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let file =
        std::fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {}", e))?;
    let decoder =
        flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?;

    emit_progress(app_handle, dep_id, "extracting", 0.1, "正在解压...");

    if mappings.is_empty() {
        // Extract everything
        for entry in entries {
            let mut entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
            let path = entry
                .path()
                .map_err(|e| format!("Tar path error: {}", e))?
                .into_owned();

            let out_path = target_dir.join(&path);
            if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(&out_path)
                    .map_err(|e| format!("Create dir error: {}", e))?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Create dir error: {}", e))?;
                }
                let mut out_file = std::fs::File::create(&out_path)
                    .map_err(|e| format!("Create file error: {}", e))?;
                io::copy(&mut entry, &mut out_file)
                    .map_err(|e| format!("Extract file error: {}", e))?;

                #[cfg(unix)]
                set_executable(&out_path);
            }
        }
    } else {
        // With mappings
        let mut matched = 0;
        for entry in entries {
            let mut entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
            if entry.header().entry_type().is_dir() {
                continue;
            }

            let entry_path = entry
                .path()
                .map_err(|e| format!("Tar path error: {}", e))?
                .to_string_lossy()
                .to_string();

            for mapping in mappings {
                if glob_matches(&entry_path, mapping.archive_glob) {
                    let out_path = target_dir.join(mapping.target_name);
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("Create dir error: {}", e))?;
                    }
                    let mut out_file = std::fs::File::create(&out_path)
                        .map_err(|e| format!("Create file error: {}", e))?;
                    io::copy(&mut entry, &mut out_file)
                        .map_err(|e| format!("Extract file error: {}", e))?;

                    #[cfg(unix)]
                    set_executable(&out_path);

                    matched += 1;
                    tracing::info!("Extracted: {} -> {}", entry_path, mapping.target_name);
                    break;
                }
            }
        }

        if matched == 0 {
            return Err("No files matched the extract mappings".to_string());
        }
    }

    emit_progress(app_handle, dep_id, "extracting", 1.0, "解压完成");
    Ok(())
}

// ==================== Helpers ====================

/// Simple glob matching: supports `*` as wildcard for path segments
/// e.g. "*/bin/ffmpeg.exe" matches "ffmpeg-7.0/bin/ffmpeg.exe"
fn glob_matches(path: &str, pattern: &str) -> bool {
    // Normalize separators
    let path = path.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");

    let path_parts: Vec<&str> = path.split('/').collect();
    let pattern_parts: Vec<&str> = pattern.split('/').collect();

    if path_parts.len() != pattern_parts.len() {
        return false;
    }

    path_parts
        .iter()
        .zip(pattern_parts.iter())
        .all(|(p, pat)| *pat == "*" || *p == *pat)
}

/// Format bytes to human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Set executable permission on Unix platforms
#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut perms = metadata.permissions();
        perms.set_mode(perms.mode() | 0o111);
        let _ = std::fs::set_permissions(path, perms);
    }
}

// ==================== Python Package Install ====================

/// Detect python3 on the system, returns the full path if found
pub fn detect_python3() -> Option<String> {
    for name in &["python3", "python"] {
        // Try running --version directly to verify it works
        if let Ok(output) = std::process::Command::new(name)
            .args(["--version"])
            .output()
        {
            if output.status.success() {
                let ver_str = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                );
                if ver_str.contains("Python 3") {
                    // Resolve the full path via `which`
                    if let Ok(which_out) = std::process::Command::new("which").arg(name).output() {
                        if which_out.status.success() {
                            let path = String::from_utf8_lossy(&which_out.stdout).trim().to_string();
                            if !path.is_empty() {
                                tracing::info!("Detected Python3: {} ({})", path, ver_str.trim());
                                return Some(path);
                            }
                        }
                    }
                    // Fallback: use the name directly
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Install a Python package into a venv via pip
pub fn install_python_package(
    python3_path: &str,
    venv_dir: &Path,
    pip_package: &str,
    proxy_url: Option<&str>,
    dep_id: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    // Step 1: Create venv
    emit_progress(app_handle, dep_id, "installing", 0.1, "正在创建 Python 虚拟环境...");
    tracing::info!("Creating venv at {} with {}", venv_dir.display(), python3_path);

    let output = std::process::Command::new(python3_path)
        .args(["-m", "venv", &venv_dir.to_string_lossy()])
        .output()
        .map_err(|e| format!("Failed to run python3 -m venv: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create Python venv: {}", stderr));
    }

    // Step 2: pip install
    emit_progress(app_handle, dep_id, "installing", 0.3, &format!("正在安装 {}...", pip_package));

    #[cfg(target_os = "windows")]
    let pip_path = venv_dir.join("Scripts").join("pip.exe");
    #[cfg(not(target_os = "windows"))]
    let pip_path = venv_dir.join("bin").join("pip");

    if !pip_path.exists() {
        return Err(format!("pip not found at {}", pip_path.display()));
    }

    let mut pip_args = vec!["install".to_string(), pip_package.to_string(), "--progress-bar".to_string(), "off".to_string()];
    if let Some(proxy) = proxy_url {
        if !proxy.is_empty() {
            pip_args.push("--proxy".to_string());
            pip_args.push(proxy.to_string());
        }
    }

    tracing::info!("Running pip install: {:?} {:?}", pip_path, pip_args);
    let output = std::process::Command::new(&pip_path)
        .args(&pip_args)
        .output()
        .map_err(|e| format!("Failed to run pip install: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pip install {} failed: {}", pip_package, stderr));
    }

    emit_progress(app_handle, dep_id, "installing", 1.0, "安装完成");
    tracing::info!("Python package '{}' installed successfully", pip_package);
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matches() {
        assert!(glob_matches(
            "ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe",
            "*/bin/ffmpeg.exe"
        ));
        assert!(glob_matches("ffmpeg", "ffmpeg"));
        assert!(!glob_matches("ffmpeg.exe", "ffprobe.exe"));
        assert!(!glob_matches("a/b/c.exe", "*/c.exe"));
        assert!(glob_matches("DanmakuFactory.exe", "DanmakuFactory.exe"));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10.0 MB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
    }
}
