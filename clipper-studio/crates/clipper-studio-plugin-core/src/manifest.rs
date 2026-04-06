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
        { self.windows.as_deref() }
        #[cfg(target_os = "macos")]
        { self.darwin.as_deref() }
        #[cfg(target_os = "linux")]
        { self.linux.as_deref() }
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
        return Err(format!(
            "plugin.json not found in {}",
            plugin_dir.display()
        ));
    }

    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read plugin.json: {}", e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse plugin.json: {}", e))
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
}
