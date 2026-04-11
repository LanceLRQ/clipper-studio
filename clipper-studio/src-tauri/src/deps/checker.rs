use std::path::Path;
use std::process::Command;

use super::registry::{self, DependencyDef, DepType, VersionCheck};

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

    let output = Command::new(&binary_path)
        .args(check.args)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    let re = regex::Regex::new(check.regex).ok()?;
    re.captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Validate a qwen3-asr-service installation directory
pub fn validate_asr_runtime(base_dir: &Path) -> Result<String, String> {
    let python_path = get_asr_python_path(base_dir);
    let main_path = base_dir
        .join("asr-service")
        .join("app")
        .join("main.py");

    if !python_path.exists() {
        return Err("未找到 Python 虚拟环境（venv）".to_string());
    }
    if !main_path.exists() {
        return Err("未找到 app/main.py 入口文件".to_string());
    }

    Ok("valid".to_string())
}

/// Get the python path for ASR service
fn get_asr_python_path(base_dir: &Path) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        base_dir
            .join("asr-service")
            .join("venv")
            .join("Scripts")
            .join("python.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        base_dir
            .join("asr-service")
            .join("venv")
            .join("bin")
            .join("python3")
    }
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
            Ok(version)
        }
        DepType::Runtime => {
            if def.id == "qwen3-asr" {
                validate_asr_runtime(dep_dir)?;
                Ok(None)
            } else {
                Ok(None)
            }
        }
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
        return Err(format!("Entry point '{}' not found in venv", py_source.entry_point));
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
