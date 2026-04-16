use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ==================== Static Dependency Definitions ====================

/// Dependency type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepType {
    /// Single/few executable binaries
    Binary,
    /// Complete runtime environment (Python venv + code + models)
    Runtime,
}

/// Archive format
#[derive(Debug, Clone, Copy)]
pub enum ArchiveType {
    Zip,
    TarGz,
}

/// Target platform
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    WindowsX86_64,
    MacOSArm64,
}

impl Platform {
    /// Get the current platform
    pub fn current() -> Option<Self> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            Some(Platform::WindowsX86_64)
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Some(Platform::MacOSArm64)
        }
        #[cfg(not(any(
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "macos", target_arch = "aarch64")
        )))]
        {
            None
        }
    }
}

/// Version detection command
#[derive(Debug, Clone)]
pub struct VersionCheck {
    /// Binary to execute (within the dep's install dir)
    pub binary: &'static str,
    /// Command arguments, e.g. ["-version"]
    pub args: &'static [&'static str],
    /// Regex to extract version from output
    pub regex: &'static str,
}

/// File mapping: archive_path -> target_path (relative to dep install dir)
#[derive(Debug, Clone)]
pub struct ExtractMapping {
    /// Glob pattern matching file(s) inside the archive
    pub archive_glob: &'static str,
    /// Target filename in the dep install directory
    pub target_name: &'static str,
}

/// Download source for a specific platform
#[derive(Debug, Clone)]
pub struct DownloadSource {
    pub platform: Platform,
    pub url: &'static str,
    pub archive_type: ArchiveType,
    pub extract_mappings: &'static [ExtractMapping],
}

/// Python package source for a specific platform (installed via venv + pip)
#[derive(Debug, Clone)]
pub struct PythonPackageSource {
    pub platform: Platform,
    /// pip package name (e.g. "dmconvert")
    pub pip_package: &'static str,
    /// Entry point script name in venv/bin/ (e.g. "dmconvert")
    pub entry_point: &'static str,
}

/// Static dependency definition (compile-time registered)
#[derive(Debug, Clone)]
pub struct DependencyDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub dep_type: DepType,
    /// Binary names to verify after installation
    pub binaries: &'static [&'static str],
    pub version_check: Option<VersionCheck>,
    /// Minimum required version (e.g. "4.4.0"), None means any version is accepted
    pub min_version: Option<&'static str>,
    pub sources: &'static [DownloadSource],
    /// Python package sources (alternative install method, typically for macOS)
    pub python_sources: &'static [PythonPackageSource],
    /// Manual download URL (fallback for users who can't access auto-download sources)
    pub manual_download_url: Option<&'static str>,
}

// ==================== Static Registry ====================

pub static DEPENDENCY_DEFS: &[DependencyDef] = &[
    DependencyDef {
        id: "ffmpeg",
        name: "FFmpeg + FFprobe",
        description: "视频切片、转码、音量提取、弹幕压制所需",
        required: true,
        dep_type: DepType::Binary,
        binaries: &["ffmpeg", "ffprobe"],
        version_check: Some(VersionCheck {
            binary: "ffmpeg",
            args: &["-version"],
            regex: r"ffmpeg version (\S+)",
        }),
        min_version: Some("4.4.0"),
        manual_download_url: Some("https://ffmpeg.org/download.html"),
        python_sources: &[],
        sources: &[
            DownloadSource {
                platform: Platform::WindowsX86_64,
                url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
                archive_type: ArchiveType::Zip,
                extract_mappings: &[
                    ExtractMapping {
                        archive_glob: "*/bin/ffmpeg.exe",
                        target_name: "ffmpeg.exe",
                    },
                    ExtractMapping {
                        archive_glob: "*/bin/ffprobe.exe",
                        target_name: "ffprobe.exe",
                    },
                ],
            },
            DownloadSource {
                platform: Platform::MacOSArm64,
                // evermeet.cx provides ffmpeg and ffprobe as separate downloads
                url: "https://evermeet.cx/ffmpeg/getrelease/zip",
                archive_type: ArchiveType::Zip,
                extract_mappings: &[
                    ExtractMapping {
                        archive_glob: "ffmpeg",
                        target_name: "ffmpeg",
                    },
                ],
            },
            DownloadSource {
                platform: Platform::MacOSArm64,
                // ffprobe separate download from evermeet.cx
                url: "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip",
                archive_type: ArchiveType::Zip,
                extract_mappings: &[
                    ExtractMapping {
                        archive_glob: "ffprobe",
                        target_name: "ffprobe",
                    },
                ],
            },
        ],
    },
    DependencyDef {
        id: "danmaku-factory",
        name: "弹幕转换工具",
        description: "弹幕 XML 转 ASS 字幕文件",
        required: false,
        dep_type: DepType::Binary,
        binaries: &["DanmakuFactory"],
        version_check: Some(VersionCheck {
            binary: "DanmakuFactory",
            args: &["--version"],
            regex: r"(\d+\.\d+\S*)",
        }),
        min_version: None,
        manual_download_url: Some("https://hihkm.lanzoui.com/b01hgf1xe"),
        python_sources: &[
            PythonPackageSource {
                platform: Platform::MacOSArm64,
                pip_package: "dmconvert",
                entry_point: "dmconvert",
            },
        ],
        sources: &[
            DownloadSource {
                platform: Platform::WindowsX86_64,
                url: "https://github.com/hihkm/DanmakuFactory/releases/download/v1.70/DanmakuFactory1.70_Release_CLI.zip",
                archive_type: ArchiveType::Zip,
                extract_mappings: &[
                    ExtractMapping {
                        archive_glob: "DanmakuFactory_REL1.70CLI.exe",
                        target_name: "DanmakuFactory.exe",
                    },
                ],
            },
        ],
    },
];

/// Look up a dependency definition by ID
pub fn get_def(id: &str) -> Option<&'static DependencyDef> {
    DEPENDENCY_DEFS.iter().find(|d| d.id == id)
}

/// Get all download sources for the current platform (may be multiple for e.g. ffmpeg + ffprobe)
pub fn get_sources_for_current_platform(def: &DependencyDef) -> Vec<&DownloadSource> {
    let platform = match Platform::current() {
        Some(p) => p,
        None => return Vec::new(),
    };
    def.sources.iter().filter(|s| s.platform == platform).collect()
}

/// Get Python package source for the current platform (if any)
pub fn get_python_source_for_current_platform(def: &DependencyDef) -> Option<&PythonPackageSource> {
    let platform = Platform::current()?;
    def.python_sources.iter().find(|s| s.platform == platform)
}

/// Check if auto-install is available for the current platform
pub fn has_source_for_current_platform(def: &DependencyDef) -> bool {
    !get_sources_for_current_platform(def).is_empty()
        || get_python_source_for_current_platform(def).is_some()
}

// ==================== Local State Registry (registry.json) ====================

/// Installed dependency state (persisted in registry.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledDepState {
    pub status: DepStatus,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Dependency installation status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepStatus {
    NotInstalled,
    Downloading,
    Installing,
    Installed,
    Error,
}

/// The local registry persisted as registry.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRegistry {
    pub version: u32,
    pub deps: HashMap<String, InstalledDepState>,
}

impl Default for LocalRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            deps: HashMap::new(),
        }
    }
}

impl LocalRegistry {
    /// Load from file, or create empty if not exists
    pub fn load(deps_dir: &Path) -> Self {
        let path = deps_dir.join("registry.json");
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<LocalRegistry>(&content) {
                    Ok(registry) => return registry,
                    Err(e) => {
                        tracing::warn!("Failed to parse registry.json: {}", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read registry.json: {}", e);
                }
            }
        }
        Self::default()
    }

    /// Save to file
    pub fn save(&self, deps_dir: &Path) -> Result<(), String> {
        let path = deps_dir.join("registry.json");
        let content =
            serde_json::to_string_pretty(self).map_err(|e| format!("JSON serialize error: {}", e))?;
        std::fs::write(&path, content).map_err(|e| format!("Write registry.json failed: {}", e))?;
        Ok(())
    }

    /// Get state for a dependency
    pub fn get(&self, dep_id: &str) -> Option<&InstalledDepState> {
        self.deps.get(dep_id)
    }

    /// Update state for a dependency
    pub fn set(&mut self, dep_id: &str, state: InstalledDepState) {
        self.deps.insert(dep_id.to_string(), state);
    }

    /// Remove state for a dependency
    pub fn remove(&mut self, dep_id: &str) {
        self.deps.remove(dep_id);
    }
}

// ==================== Combined Status (returned to frontend) ====================

/// Full dependency status returned to frontend
#[derive(Debug, Clone, Serialize)]
pub struct DependencyStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub required: bool,
    pub dep_type: DepType,
    /// Installation status in deps manager
    pub status: DepStatus,
    pub version: Option<String>,
    pub installed_path: Option<String>,
    pub custom_path: Option<String>,
    pub error_message: Option<String>,
    /// Whether auto-install is available on the current platform
    pub auto_install_available: bool,
    /// Manual download URL (fallback for users who can't access auto-download sources)
    pub manual_download_url: Option<String>,
    /// Whether already found via config.toml / bin dir / system PATH (outside deps manager)
    pub system_available: bool,
    /// The path where it was found in system (if system_available)
    pub system_path: Option<String>,
    /// Version detected from system installation
    pub system_version: Option<String>,
}

/// System detection result
pub struct SystemDetection {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

impl Default for SystemDetection {
    fn default() -> Self {
        Self {
            available: false,
            path: None,
            version: None,
        }
    }
}

/// Build a DependencyStatus from a static def + local state + config override + system detection
pub fn build_status(
    def: &DependencyDef,
    state: Option<&InstalledDepState>,
    custom_path: Option<String>,
    system: SystemDetection,
) -> DependencyStatus {
    let auto_install_available = has_source_for_current_platform(def);

    let (status, version, installed_path, error_message) = match state {
        Some(s) => (
            s.status.clone(),
            s.version.clone(),
            s.path.clone(),
            s.error_message.clone(),
        ),
        None => (DepStatus::NotInstalled, None, None, None),
    };

    // Platform-specific display name and description
    let is_python = get_python_source_for_current_platform(def).is_some();
    let name = if is_python && def.id == "danmaku-factory" {
        "DanmakuConvert".to_string()
    } else if !is_python && def.id == "danmaku-factory" {
        "DanmakuFactory".to_string()
    } else {
        def.name.to_string()
    };
    let description = if is_python && def.id == "danmaku-factory" {
        "弹幕 XML 转 ASS 字幕文件（Python）".to_string()
    } else {
        def.description.to_string()
    };

    DependencyStatus {
        id: def.id.to_string(),
        name,
        description,
        required: def.required,
        dep_type: def.dep_type,
        status,
        version,
        installed_path,
        custom_path,
        error_message,
        auto_install_available,
        manual_download_url: def.manual_download_url.map(|s| s.to_string()),
        system_available: system.available,
        system_path: system.path,
        system_version: system.version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== get_def =====

    #[test]
    fn test_get_def_existing() {
        assert!(get_def("ffmpeg").is_some());
        assert!(get_def("danmaku-factory").is_some());
    }

    #[test]
    fn test_get_def_nonexistent() {
        assert!(get_def("nonexistent").is_none());
        assert!(get_def("").is_none());
    }

    #[test]
    fn test_get_def_fields() {
        let def = get_def("ffmpeg").unwrap();
        assert_eq!(def.id, "ffmpeg");
        assert!(def.required);
        assert_eq!(def.dep_type, DepType::Binary);
        assert!(!def.sources.is_empty());
    }

    // ===== LocalRegistry =====

    #[test]
    fn test_local_registry_default() {
        let registry = LocalRegistry::default();
        assert_eq!(registry.version, 1);
        assert!(registry.deps.is_empty());
    }

    #[test]
    fn test_local_registry_set_get_remove() {
        let mut registry = LocalRegistry::default();
        assert!(registry.get("ffmpeg").is_none());

        registry.set(
            "ffmpeg",
            InstalledDepState {
                status: DepStatus::Installed,
                version: Some("7.0".to_string()),
                installed_at: Some("2026-04-05".to_string()),
                path: Some("/path/to/ffmpeg".to_string()),
                error_message: None,
            },
        );
        assert!(registry.get("ffmpeg").is_some());
        assert_eq!(registry.get("ffmpeg").unwrap().status, DepStatus::Installed);
        assert_eq!(
            registry.get("ffmpeg").unwrap().version.as_deref(),
            Some("7.0")
        );

        registry.remove("ffmpeg");
        assert!(registry.get("ffmpeg").is_none());
    }

    #[test]
    fn test_local_registry_overwrite() {
        let mut registry = LocalRegistry::default();
        registry.set(
            "ffmpeg",
            InstalledDepState {
                status: DepStatus::Downloading,
                version: None,
                installed_at: None,
                path: None,
                error_message: None,
            },
        );
        assert_eq!(registry.get("ffmpeg").unwrap().status, DepStatus::Downloading);

        registry.set(
            "ffmpeg",
            InstalledDepState {
                status: DepStatus::Installed,
                version: Some("7.0".to_string()),
                installed_at: None,
                path: None,
                error_message: None,
            },
        );
        assert_eq!(registry.get("ffmpeg").unwrap().status, DepStatus::Installed);
    }

    #[test]
    fn test_local_registry_persistence() {
        let dir = std::env::temp_dir().join("clipper_test_registry_persist");
        let _ = std::fs::create_dir_all(&dir);

        let mut registry = LocalRegistry::default();
        registry.set(
            "ffmpeg",
            InstalledDepState {
                status: DepStatus::Installed,
                version: Some("7.0".to_string()),
                installed_at: None,
                path: None,
                error_message: None,
            },
        );
        registry.save(&dir).unwrap();

        let loaded = LocalRegistry::load(&dir);
        assert_eq!(loaded.version, 1);
        assert!(loaded.get("ffmpeg").is_some());
        assert_eq!(
            loaded.get("ffmpeg").unwrap().version.as_deref(),
            Some("7.0")
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_local_registry_load_missing_file() {
        let dir = std::env::temp_dir().join("clipper_test_registry_missing");
        let _ = std::fs::create_dir_all(&dir);

        let loaded = LocalRegistry::load(&dir);
        assert_eq!(loaded.version, 1);
        assert!(loaded.deps.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ===== build_status =====

    #[test]
    fn test_build_status_not_installed() {
        let def = get_def("ffmpeg").unwrap();
        let status = build_status(def, None, None, SystemDetection::default());
        assert_eq!(status.id, "ffmpeg");
        assert_eq!(status.status, DepStatus::NotInstalled);
        assert!(!status.system_available);
        assert!(status.version.is_none());
    }

    #[test]
    fn test_build_status_with_system_detection() {
        let def = get_def("ffmpeg").unwrap();
        let status = build_status(
            def,
            None,
            None,
            SystemDetection {
                available: true,
                path: Some("/usr/bin/ffmpeg".to_string()),
                version: Some("7.0".to_string()),
            },
        );
        assert!(status.system_available);
        assert_eq!(status.system_path.as_deref(), Some("/usr/bin/ffmpeg"));
        assert_eq!(status.system_version.as_deref(), Some("7.0"));
    }

    #[test]
    fn test_build_status_with_installed_state() {
        let def = get_def("ffmpeg").unwrap();
        let state = InstalledDepState {
            status: DepStatus::Installed,
            version: Some("7.0".to_string()),
            installed_at: Some("2026-04-05".to_string()),
            path: Some("/deps/ffmpeg".to_string()),
            error_message: None,
        };
        let status = build_status(def, Some(&state), None, SystemDetection::default());
        assert_eq!(status.status, DepStatus::Installed);
        assert_eq!(status.version.as_deref(), Some("7.0"));
        assert_eq!(status.installed_path.as_deref(), Some("/deps/ffmpeg"));
    }

    #[test]
    fn test_build_status_error_state() {
        let def = get_def("ffmpeg").unwrap();
        let state = InstalledDepState {
            status: DepStatus::Error,
            version: None,
            installed_at: None,
            path: None,
            error_message: Some("download failed".to_string()),
        };
        let status = build_status(def, Some(&state), None, SystemDetection::default());
        assert_eq!(status.status, DepStatus::Error);
        assert_eq!(status.error_message.as_deref(), Some("download failed"));
    }

    #[test]
    fn test_build_status_custom_path() {
        let def = get_def("ffmpeg").unwrap();
        let status = build_status(
            def,
            None,
            Some("/custom/ffmpeg".to_string()),
            SystemDetection::default(),
        );
        assert_eq!(status.custom_path.as_deref(), Some("/custom/ffmpeg"));
    }
}
