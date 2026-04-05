use std::path::{Path, PathBuf};
use std::process::Command;

/// Detect a binary (ffmpeg/ffprobe) by checking:
/// 1. Application's bin directory (bundled with installer)
/// 2. System PATH
///
/// Returns the full path string if found, None otherwise.
pub fn detect_binary(name: &str, bin_dir: &Path) -> Option<String> {
    // Check app bin directory first
    let bin_path = get_bin_path(name, bin_dir);
    if let Some(path) = bin_path {
        if path.exists() {
            tracing::debug!("{} found in bin dir: {}", name, path.display());
            return Some(path.to_string_lossy().to_string());
        }
    }

    // Fallback: check system PATH
    if let Some(path) = find_in_path(name) {
        tracing::debug!("{} found in system PATH: {}", name, path);
        return Some(path);
    }

    None
}

/// Get platform-specific binary path in the bin directory
fn get_bin_path(name: &str, bin_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Some(bin_dir.join(format!("{}.exe", name)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Some(bin_dir.join(name))
    }
}

/// Try to find a binary in the system PATH by running `which`/`where`
fn find_in_path(name: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    let cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let cmd = "which";

    Command::new(cmd)
        .arg(name)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().lines().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

/// Get FFmpeg version string
pub fn get_version(ffmpeg_path: &str) -> Option<String> {
    if ffmpeg_path.is_empty() {
        return None;
    }
    Command::new(ffmpeg_path)
        .arg("-version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.to_string()))
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_binary_nonexistent_dir() {
        let result = detect_binary("ffmpeg", &PathBuf::from("/nonexistent/path"));
        // May or may not find in PATH depending on system
        // Just ensure it doesn't panic
        let _ = result;
    }
}
