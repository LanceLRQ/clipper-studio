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
    pub sources: &'static [DownloadSource],
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
        name: "DanmakuFactory",
        description: "弹幕 XML 转 ASS 字幕文件",
        required: false,
        dep_type: DepType::Binary,
        binaries: &["DanmakuFactory"],
        version_check: Some(VersionCheck {
            binary: "DanmakuFactory",
            args: &["--version"],
            regex: r"(\d+\.\d+\S*)",
        }),
        sources: &[
            DownloadSource {
                platform: Platform::WindowsX86_64,
                url: "https://github.com/hihkm/DanmakuFactory/releases/latest/download/DanmakuFactory_win_x64.zip",
                archive_type: ArchiveType::Zip,
                extract_mappings: &[
                    ExtractMapping {
                        archive_glob: "DanmakuFactory.exe",
                        target_name: "DanmakuFactory.exe",
                    },
                ],
            },
            DownloadSource {
                platform: Platform::MacOSArm64,
                url: "https://github.com/hihkm/DanmakuFactory/releases/latest/download/DanmakuFactory_mac.zip",
                archive_type: ArchiveType::Zip,
                extract_mappings: &[
                    ExtractMapping {
                        archive_glob: "DanmakuFactory",
                        target_name: "DanmakuFactory",
                    },
                ],
            },
        ],
    },
    DependencyDef {
        id: "qwen3-asr",
        name: "qwen3-asr-service",
        description: "本地 AI 语音识别引擎（体积较大，约 2-4 GB）",
        required: false,
        dep_type: DepType::Runtime,
        binaries: &[],
        version_check: None,
        sources: &[
            // Windows: portable package maintained by LanceLRQ
            // URL TBD - will be configured later
            // macOS: manual installation only (no auto-install)
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

/// Check if auto-install is available for the current platform
pub fn has_source_for_current_platform(def: &DependencyDef) -> bool {
    !get_sources_for_current_platform(def).is_empty()
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

    DependencyStatus {
        id: def.id.to_string(),
        name: def.name.to_string(),
        description: def.description.to_string(),
        required: def.required,
        dep_type: def.dep_type,
        status,
        version,
        installed_path,
        custom_path,
        error_message,
        auto_install_available,
        system_available: system.available,
        system_path: system.path,
        system_version: system.version,
    }
}
