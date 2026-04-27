use std::path::Path;
use std::process::Command;

use super::registry::{self, DepType, DependencyDef, VersionCheck};

/// Check if a binary exists at the given path
pub fn binary_exists(dep_dir: &Path, binary_name: &str) -> bool {
    let path = get_binary_path(dep_dir, binary_name);
    path.exists()
}

/// Get platform-specific binary path within a dep directory
pub fn get_binary_path(dep_dir: &Path, binary_name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        let name = if binary_name.ends_with(".exe") {
            binary_name.to_string()
        } else {
            format!("{}.exe", binary_name)
        };
        dep_dir.join(name)
    }
    #[cfg(not(target_os = "windows"))]
    {
        dep_dir.join(binary_name)
    }
}

/// Verify all required binaries exist for a dependency
pub fn verify_binaries(dep_dir: &Path, def: &DependencyDef) -> Result<(), String> {
    for binary in def.binaries {
        if !binary_exists(dep_dir, binary) {
            return Err(format!(
                "Binary '{}' not found in {}",
                binary,
                dep_dir.display()
            ));
        }
    }
    Ok(())
}

/// Detect version by running version command
pub fn detect_version(dep_dir: &Path, check: &VersionCheck) -> Option<String> {
    let binary_path = get_binary_path(dep_dir, check.binary);
    if !binary_path.exists() {
        return None;
    }

    let output = Command::new(&binary_path).args(check.args).output().ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    let re = regex::Regex::new(check.regex).ok()?;
    re.captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Compare two semver-like version strings (e.g. "4.4.0" vs "7.1").
/// Returns true if `detected` >= `minimum`.
/// Handles versions with extra suffixes like "7.1-essentials_build".
fn version_satisfies_minimum(detected: &str, minimum: &str) -> bool {
    let parse_parts = |s: &str| -> Vec<u32> {
        // Take only the numeric prefix (e.g. "7.1-essentials" → "7.1")
        s.split(|c: char| !c.is_ascii_digit() && c != '.')
            .next()
            .unwrap_or("")
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };

    let det = parse_parts(detected);
    let min = parse_parts(minimum);

    for i in 0..min.len().max(det.len()) {
        let d = det.get(i).copied().unwrap_or(0);
        let m = min.get(i).copied().unwrap_or(0);
        if d > m {
            return true;
        }
        if d < m {
            return false;
        }
    }
    true // equal
}

/// Full health check for a dependency
pub fn health_check(dep_dir: &Path, def: &DependencyDef) -> Result<Option<String>, String> {
    // Check if this platform uses Python package install
    if let Some(py_source) = registry::get_python_source_for_current_platform(def) {
        return validate_python_package(dep_dir, py_source, def);
    }

    match def.dep_type {
        DepType::Binary => {
            verify_binaries(dep_dir, def)?;
            let version = def
                .version_check
                .as_ref()
                .and_then(|vc| detect_version(dep_dir, vc));

            // Check minimum version requirement
            if let (Some(detected), Some(min_ver)) = (&version, def.min_version) {
                if !version_satisfies_minimum(detected, min_ver) {
                    return Err(format!(
                        "{} 版本过低（当前: {}，最低要求: {}）",
                        def.name, detected, min_ver
                    ));
                }
            }

            Ok(version)
        }
        DepType::Runtime => Ok(None),
    }
}

/// Validate a Python package installed in a venv
fn validate_python_package(
    dep_dir: &Path,
    py_source: &registry::PythonPackageSource,
    def: &DependencyDef,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    let venv_bin = dep_dir.join("venv").join("Scripts");
    #[cfg(not(target_os = "windows"))]
    let venv_bin = dep_dir.join("venv").join("bin");

    if !venv_bin.exists() {
        return Err("Python venv not found".to_string());
    }

    // Check entry_point script exists
    #[cfg(target_os = "windows")]
    let script = venv_bin.join(format!("{}.exe", py_source.entry_point));
    #[cfg(not(target_os = "windows"))]
    let script = venv_bin.join(py_source.entry_point);

    if !script.exists() {
        return Err(format!(
            "Entry point '{}' not found in venv",
            py_source.entry_point
        ));
    }

    // Version check: use the entry_point as the binary
    let version = def.version_check.as_ref().and_then(|vc| {
        let output = Command::new(&script).args(vc.args).output().ok()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let re = regex::Regex::new(vc.regex).ok()?;
        re.captures(&combined)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    });

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_fake_binary(binary_name: &str) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let target_name = if cfg!(target_os = "windows") && !binary_name.ends_with(".exe") {
            format!("{}.exe", binary_name)
        } else {
            binary_name.to_string()
        };
        fs::write(dir.path().join(&target_name), b"#!/bin/sh\nexit 0\n")
            .expect("write fake binary");
        dir
    }

    #[test]
    fn test_get_binary_path_appends_exe_on_windows_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = get_binary_path(dir.path(), "ffmpeg");

        if cfg!(target_os = "windows") {
            assert!(path.to_string_lossy().ends_with("ffmpeg.exe"));
        } else {
            assert!(path.to_string_lossy().ends_with("ffmpeg"));
            assert!(!path.to_string_lossy().ends_with(".exe"));
        }
    }

    #[test]
    fn test_get_binary_path_preserves_existing_exe_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let path = get_binary_path(dir.path(), "tool.exe");
        // 任何平台下都不应出现 tool.exe.exe
        assert!(!path.to_string_lossy().ends_with(".exe.exe"));
    }

    #[test]
    fn test_binary_exists_true_when_file_present() {
        let dir = make_fake_binary("ffprobe");
        assert!(binary_exists(dir.path(), "ffprobe"));
    }

    #[test]
    fn test_binary_exists_false_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!binary_exists(dir.path(), "nonexistent"));
    }

    #[test]
    fn test_verify_binaries_ok_when_all_present() {
        let dir = tempfile::tempdir().unwrap();
        let target_a = if cfg!(target_os = "windows") {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let target_b = if cfg!(target_os = "windows") {
            "ffprobe.exe"
        } else {
            "ffprobe"
        };
        fs::write(dir.path().join(target_a), b"x").unwrap();
        fs::write(dir.path().join(target_b), b"x").unwrap();

        let def = DependencyDef {
            id: "fake",
            name: "fake",
            description: "",
            required: false,
            dep_type: DepType::Binary,
            binaries: &["ffmpeg", "ffprobe"],
            version_check: None,
            min_version: None,
            sources: &[],
            python_sources: &[],
            manual_download_url: None,
        };

        assert!(verify_binaries(dir.path(), &def).is_ok());
    }

    #[test]
    fn test_verify_binaries_err_lists_missing() {
        let dir = make_fake_binary("ffmpeg");
        // ffprobe 故意不创建
        let def = DependencyDef {
            id: "fake",
            name: "fake",
            description: "",
            required: false,
            dep_type: DepType::Binary,
            binaries: &["ffmpeg", "ffprobe"],
            version_check: None,
            min_version: None,
            sources: &[],
            python_sources: &[],
            manual_download_url: None,
        };

        let err = verify_binaries(dir.path(), &def).expect_err("should fail");
        assert!(
            err.contains("ffprobe"),
            "错误信息应指明缺失的 binary：{}",
            err
        );
    }

    #[test]
    fn test_detect_version_returns_none_when_binary_missing() {
        let dir = tempfile::tempdir().unwrap();
        let vc = VersionCheck {
            binary: "ghost",
            args: &["-version"],
            regex: r"v(\d+\.\d+)",
        };
        assert!(detect_version(dir.path(), &vc).is_none());
    }

    // ---------- version_satisfies_minimum ----------

    #[test]
    fn test_version_equal() {
        assert!(version_satisfies_minimum("4.4.0", "4.4.0"));
    }

    #[test]
    fn test_version_higher_passes() {
        assert!(version_satisfies_minimum("7.1", "4.4.0"));
        assert!(version_satisfies_minimum("5.0", "4.4.0"));
        assert!(version_satisfies_minimum("4.4.1", "4.4.0"));
    }

    #[test]
    fn test_version_lower_fails() {
        assert!(!version_satisfies_minimum("4.3.9", "4.4.0"));
        assert!(!version_satisfies_minimum("3.9", "4.4.0"));
        assert!(!version_satisfies_minimum("4.4.0", "4.4.1"));
    }

    #[test]
    fn test_version_handles_extra_suffix() {
        // "7.1-essentials_build" 取数字前缀 "7.1"
        assert!(version_satisfies_minimum("7.1-essentials_build", "4.4.0"));
        assert!(version_satisfies_minimum("4.4.0-static", "4.4.0"));
    }

    #[test]
    fn test_version_short_minor_treated_as_zero() {
        // "5" 视作 5.0.0，应 >= 4.4.0
        assert!(version_satisfies_minimum("5", "4.4.0"));
        // "4" 视作 4.0.0，应 < 4.4.0
        assert!(!version_satisfies_minimum("4", "4.4.0"));
    }

    #[test]
    fn test_version_garbage_treated_as_zero() {
        // 完全无法解析 → 0.0.0 → < 4.4.0
        assert!(!version_satisfies_minimum("abc", "4.4.0"));
        // detected/minimum 都解析为空 → 视为相等
        assert!(version_satisfies_minimum("abc", "xyz"));
    }
}
