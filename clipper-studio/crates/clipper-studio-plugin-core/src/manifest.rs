use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Plugin transport protocol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Http,
    Stdio,
    /// Builtin plugin (compiled into the main application)
    Builtin,
}

/// Plugin type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    AsrEngine,
    LlmProvider,
    Recorder,
    Uploader,
    SyncProvider,
    WorkspaceAdapter,
    DanmakuSource,
    DanmakuRenderer,
    Exporter,
    StorageProvider,
}

/// Platform-specific executable or startup command
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformCommand {
    #[serde(default)]
    pub windows: Option<String>,
    #[serde(default)]
    pub darwin: Option<String>,
    #[serde(default)]
    pub linux: Option<String>,
}

impl PlatformCommand {
    /// Get the command for the current platform
    pub fn current(&self) -> Option<&str> {
        #[cfg(target_os = "windows")]
        {
            self.windows.as_deref()
        }
        #[cfg(target_os = "macos")]
        {
            self.darwin.as_deref()
        }
        #[cfg(target_os = "linux")]
        {
            self.linux.as_deref()
        }
    }
}

/// Plugin dependency declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    pub id: String,
    /// Semver version requirement (e.g. ">=1.0.0")
    #[serde(default)]
    pub version: Option<String>,
}

/// Plugin conflict declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConflict {
    pub id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The plugin.json manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub version: String,
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    pub transport: Transport,
    /// Whether this plugin is a managed subprocess (started/stopped by ClipperStudio)
    #[serde(default)]
    pub managed: bool,
    /// Whether only one instance of this plugin type can be active
    #[serde(default)]
    pub singleton: bool,
    /// Startup command (for managed service plugins)
    #[serde(default)]
    pub startup: Option<PlatformCommand>,
    /// Executable path (for stdio plugins)
    #[serde(default)]
    pub executable: Option<PlatformCommand>,
    /// Health check endpoint (for HTTP service plugins)
    #[serde(default)]
    pub health_endpoint: Option<String>,
    /// Default port (for HTTP service plugins)
    #[serde(default)]
    pub port: Option<u16>,
    /// Configuration schema for dynamic form generation
    #[serde(default)]
    pub config_schema: HashMap<String, serde_json::Value>,
    /// Plugin dependencies
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    /// Plugin conflicts
    #[serde(default)]
    pub conflicts: Vec<PluginConflict>,
    /// Plugin description
    #[serde(default)]
    pub description: Option<String>,
    /// Frontend entry for plugin UI (path relative to plugin dir)
    #[serde(default)]
    pub frontend: Option<PluginFrontend>,
}

/// Frontend entry configuration for plugin UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFrontend {
    /// Path to the JS bundle (relative to plugin directory)
    pub entry: String,
    /// Target mount point in settings page
    #[serde(default = "default_frontend_target")]
    pub target: String,
}

fn default_frontend_target() -> String {
    "settings".to_string()
}

fn default_api_version() -> u32 {
    1
}

/// Loaded plugin metadata (manifest + resolved path)
#[derive(Debug, Clone)]
pub struct PluginMeta {
    pub manifest: PluginManifest,
    /// Directory containing the plugin (empty for builtin plugins)
    pub dir: PathBuf,
    /// Current runtime status
    pub status: PluginStatus,
}

/// Plugin runtime status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    /// Discovered but not loaded
    Discovered,
    /// Loaded and ready
    Loaded,
    /// Service is running (for managed service plugins)
    Running,
    /// Plugin has an error
    Error,
    /// Disabled by user
    Disabled,
}

/// Load a plugin manifest from a directory
pub fn load_manifest(plugin_dir: &Path) -> Result<PluginManifest, String> {
    let manifest_path = plugin_dir.join("plugin.json");
    if !manifest_path.exists() {
        return Err(format!("plugin.json not found in {}", plugin_dir.display()));
    }

    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read plugin.json: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse plugin.json: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_asr_manifest() {
        let json = r#"{
            "id": "asr.qwen3",
            "name": "Qwen3 ASR",
            "type": "asr-engine",
            "version": "2.0.0",
            "api_version": 1,
            "transport": "http",
            "managed": true,
            "health_endpoint": "/v1/health",
            "port": 8765,
            "config_schema": {
                "device": { "type": "string", "default": "auto" }
            }
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.id, "asr.qwen3");
        assert_eq!(manifest.plugin_type, PluginType::AsrEngine);
        assert_eq!(manifest.transport, Transport::Http);
        assert!(manifest.managed);
        assert_eq!(manifest.port, Some(8765));
    }

    #[test]
    fn test_parse_stdio_manifest() {
        let json = r#"{
            "id": "danmaku.bilibili",
            "name": "Bilibili Danmaku",
            "type": "danmaku-renderer",
            "version": "1.0.0",
            "transport": "stdio",
            "executable": {
                "darwin": "converter",
                "linux": "converter"
            }
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.plugin_type, PluginType::DanmakuRenderer);
        assert_eq!(manifest.transport, Transport::Stdio);
        assert_eq!(manifest.api_version, 1); // default
        assert!(!manifest.managed); // default false
    }

    #[test]
    fn test_parse_builtin_manifest() {
        let json = r#"{
            "id": "recorder.bililive",
            "name": "BililiveRecorder",
            "type": "recorder",
            "version": "1.0.0",
            "transport": "builtin",
            "singleton": true
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.transport, Transport::Builtin);
        assert!(manifest.singleton);
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.conflicts.is_empty());
    }

    #[test]
    fn test_parse_with_dependencies_and_conflicts() {
        let json = r#"{
            "id": "uploader.bilibili",
            "name": "Bilibili Uploader",
            "type": "uploader",
            "version": "1.0.0",
            "transport": "http",
            "dependencies": [
                { "id": "asr.qwen3", "version": ">=2.0.0" },
                { "id": "danmaku.bilibili" }
            ],
            "conflicts": [
                { "id": "uploader.douyin", "reason": "互斥的上传通道" }
            ]
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.dependencies.len(), 2);
        assert_eq!(m.dependencies[0].id, "asr.qwen3");
        assert_eq!(m.dependencies[0].version, Some(">=2.0.0".to_string()));
        assert!(m.dependencies[1].version.is_none());
        assert_eq!(m.conflicts.len(), 1);
        assert_eq!(m.conflicts[0].id, "uploader.douyin");
    }

    #[test]
    fn test_plugin_type_all_variants_round_trip() {
        // kebab-case serialization for every variant
        let cases = [
            (PluginType::AsrEngine, "\"asr-engine\""),
            (PluginType::LlmProvider, "\"llm-provider\""),
            (PluginType::Recorder, "\"recorder\""),
            (PluginType::Uploader, "\"uploader\""),
            (PluginType::SyncProvider, "\"sync-provider\""),
            (PluginType::WorkspaceAdapter, "\"workspace-adapter\""),
            (PluginType::DanmakuSource, "\"danmaku-source\""),
            (PluginType::DanmakuRenderer, "\"danmaku-renderer\""),
            (PluginType::Exporter, "\"exporter\""),
            (PluginType::StorageProvider, "\"storage-provider\""),
        ];
        for (variant, expected) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected);
            let parsed: PluginType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn test_transport_round_trip() {
        for v in [Transport::Http, Transport::Stdio, Transport::Builtin] {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: Transport = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, v);
        }
    }

    #[test]
    fn test_plugin_status_round_trip() {
        for v in [
            PluginStatus::Discovered,
            PluginStatus::Loaded,
            PluginStatus::Running,
            PluginStatus::Error,
            PluginStatus::Disabled,
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: PluginStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, v);
        }
    }

    #[test]
    fn test_platform_command_current_returns_correct_platform() {
        let cmd = PlatformCommand {
            windows: Some("win.exe".to_string()),
            darwin: Some("mac".to_string()),
            linux: Some("linux".to_string()),
        };
        let current = cmd.current().unwrap();
        #[cfg(target_os = "windows")]
        assert_eq!(current, "win.exe");
        #[cfg(target_os = "macos")]
        assert_eq!(current, "mac");
        #[cfg(target_os = "linux")]
        assert_eq!(current, "linux");
    }

    #[test]
    fn test_platform_command_current_returns_none_when_missing() {
        let cmd = PlatformCommand::default();
        assert!(cmd.current().is_none());
    }

    #[test]
    fn test_parse_with_frontend() {
        let json = r#"{
            "id": "p",
            "name": "P",
            "type": "exporter",
            "version": "0.1.0",
            "transport": "builtin",
            "frontend": { "entry": "index.js" }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        let fe = m.frontend.expect("frontend should parse");
        assert_eq!(fe.entry, "index.js");
        assert_eq!(fe.target, "settings"); // default
    }

    #[test]
    fn test_parse_invalid_json_errors() {
        let bad = r#"{ "id": "p", "name": }"#;
        let result: Result<PluginManifest, _> = serde_json::from_str(bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_field() {
        // version required, omitted
        let json = r#"{
            "id": "p", "name": "P", "type": "recorder", "transport": "builtin"
        }"#;
        let result: Result<PluginManifest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // ==================== load_manifest (file IO) ====================

    #[test]
    fn test_load_manifest_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest_path = tmp.path().join("plugin.json");
        std::fs::write(
            &manifest_path,
            r#"{
                "id": "test.plugin",
                "name": "Test Plugin",
                "type": "exporter",
                "version": "0.1.0",
                "transport": "builtin"
            }"#,
        )
        .unwrap();

        let m = load_manifest(tmp.path()).expect("load should succeed");
        assert_eq!(m.id, "test.plugin");
        assert_eq!(m.plugin_type, PluginType::Exporter);
    }

    #[test]
    fn test_load_manifest_missing_file_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let result = load_manifest(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("plugin.json not found"));
    }

    #[test]
    fn test_load_manifest_invalid_json_errors() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("plugin.json"), b"{ not valid json }").unwrap();
        let result = load_manifest(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }
}
