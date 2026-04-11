use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Application configuration loaded from config.toml
///
/// Stored at `{app_data_dir}/config.toml`, user-editable.
/// Changes take effect after app restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ffmpeg: FfmpegConfig,
    #[serde(default)]
    pub workspaces: WorkspacesConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    /// Override DanmakuFactory binary path (empty = auto-detect)
    #[serde(default)]
    pub danmaku_factory_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    /// HTTP proxy URL for dependency downloads (e.g. "http://127.0.0.1:7890")
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Database file path (relative to config dir, or absolute)
    /// Default: "data.db" (same directory as config.toml)
    #[serde(default = "default_database_path")]
    pub database_path: String,

    /// Log level: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegConfig {
    /// Override FFmpeg binary path (empty = auto-detect)
    #[serde(default)]
    pub ffmpeg_path: String,

    /// Override FFprobe binary path (empty = auto-detect)
    #[serde(default)]
    pub ffprobe_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacesConfig {
    /// Recently opened workspace paths (maintained by the app, also user-editable)
    #[serde(default)]
    pub recent: Vec<String>,
}

// Default value functions
fn default_database_path() -> String {
    "data.db".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            database_path: default_database_path(),
            log_level: default_log_level(),
        }
    }
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
        }
    }
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            recent: Vec::new(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            ffmpeg: FfmpegConfig::default(),
            workspaces: WorkspacesConfig::default(),
            tools: ToolsConfig::default(),
            network: NetworkConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load config from file, or create default if not exists
    pub fn load(config_dir: &Path) -> Self {
        let config_path = config_dir.join("config.toml");

        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match toml::from_str::<AppConfig>(&content) {
                    Ok(config) => {
                        tracing::info!("Config loaded from {}", config_path.display());
                        return config;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse config.toml, using defaults: {}",
                            e
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to read config.toml, using defaults: {}",
                        e
                    );
                }
            }
        }

        // Create default config file
        let config = AppConfig::default();
        if let Err(e) = config.save(config_dir) {
            tracing::warn!("Failed to write default config.toml: {}", e);
        }
        config
    }

    /// Save config to file
    pub fn save(&self, config_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = config_dir.join("config.toml");
        let header = "# ClipperStudio 配置文件\n\
                      # 修改后重启应用生效\n\n";
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_path, format!("{}{}", header, content))?;
        tracing::debug!("Config saved to {}", config_path.display());
        Ok(())
    }

    /// Resolve database path (relative to config_dir or absolute)
    pub fn resolve_db_path(&self, config_dir: &Path) -> PathBuf {
        let db_path = Path::new(&self.general.database_path);
        if db_path.is_absolute() {
            db_path.to_path_buf()
        } else {
            config_dir.join(db_path)
        }
    }

    /// Add a workspace path to recent list (deduplicated, most recent first)
    pub fn add_recent_workspace(&mut self, path: &str) {
        self.workspaces.recent.retain(|p| p != path);
        self.workspaces.recent.insert(0, path.to_string());
        // Keep max 20 recent entries
        self.workspaces.recent.truncate(20);
    }

    /// Remove a workspace path from recent list
    pub fn remove_recent_workspace(&mut self, path: &str) {
        self.workspaces.recent.retain(|p| p != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.general.database_path, "data.db");
        assert_eq!(config.general.log_level, "info");
        assert!(config.ffmpeg.ffmpeg_path.is_empty());
        assert!(config.workspaces.recent.is_empty());
    }

    #[test]
    fn test_load_creates_default() {
        let tmp = TempDir::new().unwrap();
        let config = AppConfig::load(tmp.path());
        assert_eq!(config.general.log_level, "info");
        // Verify file was created
        assert!(tmp.path().join("config.toml").exists());
    }

    #[test]
    fn test_load_custom_config() {
        let tmp = TempDir::new().unwrap();
        let mut f = fs::File::create(tmp.path().join("config.toml")).unwrap();
        writeln!(f, "[general]\nlog_level = \"debug\"\n\n[ffmpeg]\nffmpeg_path = \"/usr/local/bin/ffmpeg\"").unwrap();

        let config = AppConfig::load(tmp.path());
        assert_eq!(config.general.log_level, "debug");
        assert_eq!(config.ffmpeg.ffmpeg_path, "/usr/local/bin/ffmpeg");
    }

    #[test]
    fn test_resolve_db_path_relative() {
        let config = AppConfig::default();
        let resolved = config.resolve_db_path(Path::new("/app/data"));
        assert_eq!(resolved, PathBuf::from("/app/data/data.db"));
    }

    #[test]
    fn test_recent_workspaces() {
        let mut config = AppConfig::default();
        config.add_recent_workspace("/path/a");
        config.add_recent_workspace("/path/b");
        config.add_recent_workspace("/path/a"); // duplicate, should move to front
        assert_eq!(config.workspaces.recent, vec!["/path/a", "/path/b"]);

        config.remove_recent_workspace("/path/b");
        assert_eq!(config.workspaces.recent, vec!["/path/a"]);
    }

    #[test]
    fn test_recent_workspaces_truncate_at_20() {
        let mut config = AppConfig::default();
        for i in 0..25 {
            config.add_recent_workspace(&format!("/path/{}", i));
        }
        assert_eq!(config.workspaces.recent.len(), 20);
        // Most recent should be first
        assert_eq!(config.workspaces.recent[0], "/path/24");
    }

    #[test]
    fn test_recent_workspaces_dedup_moves_to_front() {
        let mut config = AppConfig::default();
        config.add_recent_workspace("/path/a");
        config.add_recent_workspace("/path/b");
        config.add_recent_workspace("/path/c");
        // Re-add "a" — should move to front
        config.add_recent_workspace("/path/a");
        assert_eq!(config.workspaces.recent, vec!["/path/a", "/path/c", "/path/b"]);
    }

    #[test]
    fn test_remove_recent_nonexistent() {
        let mut config = AppConfig::default();
        config.add_recent_workspace("/path/a");
        config.remove_recent_workspace("/path/nonexistent");
        // Should not panic, list unchanged
        assert_eq!(config.workspaces.recent, vec!["/path/a"]);
    }

    #[test]
    fn test_resolve_db_path_absolute() {
        let mut config = AppConfig::default();
        config.general.database_path = "/absolute/path/my.db".to_string();
        let resolved = config.resolve_db_path(Path::new("/app/data"));
        assert_eq!(resolved, PathBuf::from("/absolute/path/my.db"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut original = AppConfig::default();
        original.general.log_level = "debug".to_string();
        original.ffmpeg.ffmpeg_path = "/usr/local/bin/ffmpeg".to_string();
        original.add_recent_workspace("/test/path");

        original.save(tmp.path()).unwrap();

        let loaded = AppConfig::load(tmp.path());
        assert_eq!(loaded.general.log_level, "debug");
        assert_eq!(loaded.ffmpeg.ffmpeg_path, "/usr/local/bin/ffmpeg");
        assert_eq!(loaded.workspaces.recent, vec!["/test/path"]);
    }

    #[test]
    fn test_load_invalid_toml_uses_defaults() {
        let tmp = TempDir::new().unwrap();
        let mut f = fs::File::create(tmp.path().join("config.toml")).unwrap();
        writeln!(f, "this is not valid toml [[[").unwrap();

        let config = AppConfig::load(tmp.path());
        // Should fall back to defaults
        assert_eq!(config.general.log_level, "info");
    }

    #[test]
    fn test_load_partial_config_fills_defaults() {
        let tmp = TempDir::new().unwrap();
        let mut f = fs::File::create(tmp.path().join("config.toml")).unwrap();
        // Only override log_level, rest should be defaults
        writeln!(f, "[general]\nlog_level = \"trace\"").unwrap();

        let config = AppConfig::load(tmp.path());
        assert_eq!(config.general.log_level, "trace");
        assert_eq!(config.general.database_path, "data.db");
        assert!(config.ffmpeg.ffmpeg_path.is_empty());
        assert!(config.workspaces.recent.is_empty());
    }

    #[test]
    fn test_toml_serialization_roundtrip() {
        let mut config = AppConfig::default();
        config.general.log_level = "warn".to_string();
        config.ffmpeg.ffprobe_path = "/opt/bin/ffprobe".to_string();

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.general.log_level, "warn");
        assert_eq!(parsed.ffmpeg.ffprobe_path, "/opt/bin/ffprobe");
    }
}
