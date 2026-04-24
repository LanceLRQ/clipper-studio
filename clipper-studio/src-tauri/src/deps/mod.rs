use crate::utils::locks::RwLockExt;
pub mod checker;
pub mod installer;
pub mod registry;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

use tauri::{AppHandle, Emitter};

use registry::{
    build_status, get_def, get_sources_for_current_platform, DepStatus, DependencyStatus,
    InstalledDepState, LocalRegistry, SystemDetection, DEPENDENCY_DEFS,
};

use crate::utils::ffmpeg;

/// Build an HTTP client with optional proxy
fn build_http_client(proxy_url: Option<&str>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .user_agent("ClipperStudio/0.1")
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(30));

    if let Some(url) = proxy_url {
        if !url.is_empty() {
            match reqwest::Proxy::all(url) {
                Ok(proxy) => {
                    builder = builder.proxy(proxy);
                    tracing::info!("HTTP proxy configured: {}", url);
                }
                Err(e) => {
                    tracing::warn!("Invalid proxy URL '{}': {}", url, e);
                }
            }
        }
    }

    builder.build().unwrap_or_default()
}

/// Manages third-party dependency detection, installation, and removal
pub struct DependencyManager {
    deps_dir: PathBuf,
    registry: RwLock<LocalRegistry>,
    http_client: RwLock<reqwest::Client>,
    cancel_tokens: RwLock<HashMap<String, Arc<AtomicBool>>>,
}

impl DependencyManager {
    /// Create a new DependencyManager and load the local registry
    pub fn new(deps_dir: PathBuf, proxy_url: Option<&str>) -> Self {
        std::fs::create_dir_all(&deps_dir).ok();
        let registry = LocalRegistry::load(&deps_dir);
        tracing::info!(
            "DependencyManager initialized, deps_dir={}",
            deps_dir.display()
        );

        Self {
            deps_dir,
            registry: RwLock::new(registry),
            http_client: RwLock::new(build_http_client(proxy_url)),
            cancel_tokens: RwLock::new(HashMap::new()),
        }
    }

    /// Update the HTTP client with a new proxy URL
    pub fn update_proxy(&self, proxy_url: Option<&str>) {
        let new_client = build_http_client(proxy_url);
        if let Ok(mut client) = self.http_client.write() {
            *client = new_client;
        }
    }

    /// List all dependencies with their current status
    pub fn list_deps(
        &self,
        config_overrides: &ConfigOverrides,
        bin_dir: &Path,
    ) -> Vec<DependencyStatus> {
        let reg = self.registry.read_safe();
        DEPENDENCY_DEFS
            .iter()
            .map(|def| {
                let state = reg.get(def.id);
                let custom_path = config_overrides.get(def.id);
                let system = self.detect_system(def, &custom_path, bin_dir);
                build_status(def, state, custom_path, system)
            })
            .collect()
    }

    /// Check a single dependency (force re-detect, update registry)
    pub fn check_dep(
        &self,
        dep_id: &str,
        config_overrides: &ConfigOverrides,
        bin_dir: &Path,
    ) -> Result<DependencyStatus, String> {
        let def = get_def(dep_id).ok_or_else(|| format!("Unknown dependency: {}", dep_id))?;
        let dep_dir = self.deps_dir.join(dep_id);

        // Re-check installation status
        if dep_dir.exists() {
            match checker::health_check(&dep_dir, def) {
                Ok(version) => {
                    let state = InstalledDepState {
                        status: DepStatus::Installed,
                        version,
                        installed_at: None, // Preserve existing value
                        path: Some(dep_dir.to_string_lossy().to_string()),
                        error_message: None,
                    };
                    self.update_registry(dep_id, state);
                }
                Err(e) => {
                    let state = InstalledDepState {
                        status: DepStatus::Error,
                        version: None,
                        installed_at: None,
                        path: Some(dep_dir.to_string_lossy().to_string()),
                        error_message: Some(e),
                    };
                    self.update_registry(dep_id, state);
                }
            }
        } else {
            self.update_registry(
                dep_id,
                InstalledDepState {
                    status: DepStatus::NotInstalled,
                    version: None,
                    installed_at: None,
                    path: None,
                    error_message: None,
                },
            );
        }

        let reg = self.registry.read_safe();
        let state = reg.get(dep_id);
        let custom_path = config_overrides.get(dep_id);
        let system = self.detect_system(def, &custom_path, bin_dir);
        Ok(build_status(def, state, custom_path, system))
    }

    /// Install a dependency (download + extract + verify)
    pub async fn install_dep(
        &self,
        dep_id: &str,
        app_handle: &AppHandle,
        proxy_url: Option<&str>,
    ) -> Result<(), String> {
        let def = get_def(dep_id).ok_or_else(|| format!("Unknown dependency: {}", dep_id))?;

        // Check if this platform uses Python package install
        if let Some(py_source) = registry::get_python_source_for_current_platform(def) {
            return self.install_python_dep(dep_id, def, py_source, proxy_url, app_handle);
        }

        let sources = get_sources_for_current_platform(def);
        if sources.is_empty() {
            return Err(format!("当前平台没有可用的自动安装源: {}", dep_id));
        }

        // Create and register cancel token
        let cancel_token = Arc::new(AtomicBool::new(false));
        {
            let mut tokens = self.cancel_tokens.write_safe();
            tokens.insert(dep_id.to_string(), cancel_token.clone());
        }

        let result = self
            .install_dep_binary(dep_id, def, &sources, app_handle, &cancel_token)
            .await;

        // Always clean up cancel token
        {
            let mut tokens = self.cancel_tokens.write_safe();
            tokens.remove(dep_id);
        }

        result
    }

    async fn install_dep_binary(
        &self,
        dep_id: &str,
        def: &registry::DependencyDef,
        sources: &[&registry::DownloadSource],
        app_handle: &AppHandle,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        let dep_dir = self.deps_dir.join(dep_id);
        let temp_dir = self.deps_dir.join(format!(".{}-temp", dep_id));
        let staging_dir = self.deps_dir.join(format!(".{}-staging", dep_id));

        // Update status to downloading
        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::Downloading,
                version: None,
                installed_at: None,
                path: None,
                error_message: None,
            },
        );

        // Clean up temp/staging dirs if exists
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to create temp directory: {}", e))?;

        if staging_dir.exists() {
            let _ = std::fs::remove_dir_all(&staging_dir);
        }
        std::fs::create_dir_all(&staging_dir)
            .map_err(|e| format!("Failed to create staging directory: {}", e))?;

        // Download and extract each source
        let total_sources = sources.len();
        for (idx, source) in sources.iter().enumerate() {
            // Check cancellation before each source
            if cancel_token.load(Ordering::Relaxed) {
                self.cleanup_cancel(dep_id, &temp_dir, &staging_dir);
                return Err("安装已取消".to_string());
            }

            let archive_name = url_to_filename(source.url);
            let archive_path = temp_dir.join(&archive_name);

            let file_label = source
                .extract_mappings
                .first()
                .map(|m| m.target_name)
                .unwrap_or("file");
            let label = if total_sources > 1 {
                format!("{} ({}/{})", file_label, idx + 1, total_sources)
            } else {
                file_label.to_string()
            };

            // Download
            let client = self
                .http_client
                .read()
                .map_err(|e| format!("Failed to acquire HTTP client: {}", e))?
                .clone();
            let download_result = installer::download_file(
                &client,
                source.url,
                &archive_path,
                dep_id,
                &label,
                app_handle,
                Some(cancel_token.clone()),
            )
            .await;

            if let Err(e) = download_result {
                let cancelled = cancel_token.load(Ordering::Relaxed);
                self.cleanup_on_error(dep_id, &temp_dir, &staging_dir, cancelled, &e, app_handle);
                return Err(e);
            }

            // Check cancellation before extraction
            if cancel_token.load(Ordering::Relaxed) {
                self.cleanup_cancel(dep_id, &temp_dir, &staging_dir);
                return Err("安装已取消".to_string());
            }

            // Update status to installing (extracting)
            self.update_registry(
                dep_id,
                InstalledDepState {
                    status: DepStatus::Installing,
                    version: None,
                    installed_at: None,
                    path: None,
                    error_message: None,
                },
            );

            // Extract
            let extract_result = installer::extract_archive(
                &archive_path,
                &staging_dir,
                source.archive_type,
                source.extract_mappings,
                dep_id,
                app_handle,
            );

            if let Err(e) = extract_result {
                self.cleanup_on_error(dep_id, &temp_dir, &staging_dir, false, &e, app_handle);
                return Err(e);
            }
        }

        // Clean up download temp
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Verify on staging dir
        installer::emit_progress_static(app_handle, dep_id, "verifying", 0.5, "正在验证...");

        let version = match checker::health_check(&staging_dir, def) {
            Ok(v) => v,
            Err(e) => {
                let err_msg = format!("安装验证失败: {}", e);
                self.cleanup_on_error(dep_id, &std::path::PathBuf::new(), &staging_dir, false, &err_msg, app_handle);
                return Err(err_msg);
            }
        };

        // Atomic swap: remove old dep_dir, rename staging -> dep_dir
        if dep_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dep_dir) {
                let err_msg = format!("移除旧依赖目录失败: {}", e);
                self.cleanup_on_error(dep_id, &std::path::PathBuf::new(), &staging_dir, false, &err_msg, app_handle);
                return Err(err_msg);
            }
        }
        if let Err(e) = std::fs::rename(&staging_dir, &dep_dir) {
            let err_msg = format!("发布新依赖目录失败: {}", e);
            let _ = std::fs::remove_dir_all(&staging_dir);
            self.set_error(dep_id, &err_msg);
            let _ = app_handle.emit(
                "dep:install-error",
                serde_json::json!({ "dep_id": dep_id, "error": err_msg }),
            );
            return Err(err_msg);
        }

        // Success
        let now = chrono_now();
        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::Installed,
                version: version.clone(),
                installed_at: Some(now),
                path: Some(dep_dir.to_string_lossy().to_string()),
                error_message: None,
            },
        );

        let _ = app_handle.emit(
            "dep:install-complete",
            serde_json::json!({
                "dep_id": dep_id,
                "version": version,
            }),
        );

        tracing::info!("Dependency '{}' installed successfully", dep_id);
        Ok(())
    }

    /// Cancel an in-progress dependency installation
    pub fn cancel_dep(&self, dep_id: &str) -> Result<(), String> {
        let tokens = self.cancel_tokens.read_safe();
        if let Some(token) = tokens.get(dep_id) {
            token.store(true, Ordering::Relaxed);
            tracing::info!("Cancel requested for dependency '{}'", dep_id);
            Ok(())
        } else {
            Err(format!("没有正在进行的安装任务: {}", dep_id))
        }
    }

    /// Clean up on cancellation
    fn cleanup_cancel(&self, dep_id: &str, temp_dir: &Path, staging_dir: &Path) {
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(temp_dir);
        }
        if staging_dir.exists() {
            let _ = std::fs::remove_dir_all(staging_dir);
        }
        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::NotInstalled,
                version: None,
                installed_at: None,
                path: None,
                error_message: None,
            },
        );
    }

    /// Clean up on error and emit error event
    fn cleanup_on_error(
        &self,
        dep_id: &str,
        temp_dir: &Path,
        staging_dir: &Path,
        cancelled: bool,
        error: &str,
        app_handle: &AppHandle,
    ) {
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(temp_dir);
        }
        if staging_dir.exists() {
            let _ = std::fs::remove_dir_all(staging_dir);
        }

        if cancelled {
            self.update_registry(
                dep_id,
                InstalledDepState {
                    status: DepStatus::NotInstalled,
                    version: None,
                    installed_at: None,
                    path: None,
                    error_message: None,
                },
            );
        } else {
            self.set_error(dep_id, error);
        }

        let _ = app_handle.emit(
            "dep:install-error",
            serde_json::json!({ "dep_id": dep_id, "error": error }),
        );
    }

    /// Uninstall a dependency (delete files + update registry)
    pub fn uninstall_dep(&self, dep_id: &str) -> Result<(), String> {
        let _ = get_def(dep_id).ok_or_else(|| format!("Unknown dependency: {}", dep_id))?;

        let dep_dir = self.deps_dir.join(dep_id);
        if dep_dir.exists() {
            std::fs::remove_dir_all(&dep_dir)
                .map_err(|e| format!("Failed to remove dep directory: {}", e))?;
        }

        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::NotInstalled,
                version: None,
                installed_at: None,
                path: None,
                error_message: None,
            },
        );

        tracing::info!("Dependency '{}' uninstalled", dep_id);
        Ok(())
    }

    /// Get the resolved binary path for a dependency's binary
    /// Returns None if not installed via deps manager
    pub fn get_binary_path(&self, dep_id: &str, binary_name: &str) -> Option<PathBuf> {
        let dep_dir = self.deps_dir.join(dep_id);

        // For Python package deps, look in venv/bin/ for the entry_point
        if let Some(def) = get_def(dep_id) {
            if let Some(py_source) = registry::get_python_source_for_current_platform(def) {
                #[cfg(target_os = "windows")]
                let path = dep_dir
                    .join("venv")
                    .join("Scripts")
                    .join(format!("{}.exe", py_source.entry_point));
                #[cfg(not(target_os = "windows"))]
                let path = dep_dir.join("venv").join("bin").join(py_source.entry_point);
                return if path.exists() { Some(path) } else { None };
            }
        }

        let path = checker::get_binary_path(&dep_dir, binary_name);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Get the deps directory path
    pub fn deps_dir(&self) -> &Path {
        &self.deps_dir
    }

    /// Refresh all deps status (re-check each installed dep)
    pub fn refresh_all(&self) {
        for def in DEPENDENCY_DEFS {
            let dep_id = def.id;
            let dep_dir = self.deps_dir.join(dep_id);

            // Clean up stale temp/staging directories from interrupted installs
            let temp_dir = self.deps_dir.join(format!(".{}-temp", dep_id));
            let staging_dir = self.deps_dir.join(format!(".{}-staging", dep_id));
            if temp_dir.exists() {
                tracing::info!("Cleaning up stale temp dir for '{}'", dep_id);
                let _ = std::fs::remove_dir_all(&temp_dir);
            }
            if staging_dir.exists() {
                tracing::info!("Cleaning up stale staging dir for '{}'", dep_id);
                let _ = std::fs::remove_dir_all(&staging_dir);
            }

            if dep_dir.exists() {
                match checker::health_check(&dep_dir, def) {
                    Ok(version) => {
                        let reg = self.registry.read_safe();
                        let needs_update = match reg.get(dep_id) {
                            Some(s) => s.status != DepStatus::Installed,
                            None => true,
                        };
                        drop(reg);

                        if needs_update {
                            self.update_registry(
                                dep_id,
                                InstalledDepState {
                                    status: DepStatus::Installed,
                                    version,
                                    installed_at: None,
                                    path: Some(dep_dir.to_string_lossy().to_string()),
                                    error_message: None,
                                },
                            );
                            tracing::info!("Reset '{}' from stale status to Installed", dep_id);
                        }
                    }
                    Err(_) => {
                        // Dir exists but health check fails
                    }
                }
            } else {
                // Reset stuck downloading/installing status when dep_dir doesn't exist
                let reg = self.registry.read_safe();
                let needs_reset = match reg.get(dep_id) {
                    Some(s) => {
                        s.status == DepStatus::Downloading || s.status == DepStatus::Installing
                    }
                    None => false,
                };
                drop(reg);

                if needs_reset {
                    self.update_registry(
                        dep_id,
                        InstalledDepState {
                            status: DepStatus::NotInstalled,
                            version: None,
                            installed_at: None,
                            path: None,
                            error_message: None,
                        },
                    );
                    tracing::info!("Reset stuck status for '{}' to NotInstalled", dep_id);
                }
            }
        }
    }

    // ==================== Python Package Install ====================

    /// Install a dependency via Python venv + pip
    fn install_python_dep(
        &self,
        dep_id: &str,
        def: &registry::DependencyDef,
        py_source: &registry::PythonPackageSource,
        proxy_url: Option<&str>,
        app_handle: &AppHandle,
    ) -> Result<(), String> {
        // Detect python3
        let python3 = installer::detect_python3().ok_or_else(|| {
            "未找到 Python3。请安装 Python 3（brew install python3 或从 python.org 下载）"
                .to_string()
        })?;

        let dep_dir = self.deps_dir.join(dep_id);
        let venv_dir = dep_dir.join("venv");

        // Update status
        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::Installing,
                version: None,
                installed_at: None,
                path: None,
                error_message: None,
            },
        );

        // Clean existing
        if dep_dir.exists() {
            let _ = std::fs::remove_dir_all(&dep_dir);
        }
        std::fs::create_dir_all(&dep_dir)
            .map_err(|e| format!("Failed to create dep directory: {}", e))?;

        // Install via venv + pip
        if let Err(e) = installer::install_python_package(
            &python3,
            &venv_dir,
            py_source.pip_package,
            proxy_url,
            dep_id,
            app_handle,
        ) {
            self.set_error(dep_id, &e);
            let _ = std::fs::remove_dir_all(&dep_dir);
            let _ = app_handle.emit(
                "dep:install-error",
                serde_json::json!({ "dep_id": dep_id, "error": e }),
            );
            return Err(e);
        }

        // Verify
        installer::emit_progress_static(app_handle, dep_id, "verifying", 0.5, "正在验证...");
        let version = match checker::health_check(&dep_dir, def) {
            Ok(v) => v,
            Err(e) => {
                let err_msg = format!("安装验证失败: {}", e);
                self.set_error(dep_id, &err_msg);
                let _ = app_handle.emit(
                    "dep:install-error",
                    serde_json::json!({ "dep_id": dep_id, "error": err_msg }),
                );
                return Err(err_msg);
            }
        };

        // Success
        let now = chrono_now();
        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::Installed,
                version: version.clone(),
                installed_at: Some(now),
                path: Some(dep_dir.to_string_lossy().to_string()),
                error_message: None,
            },
        );

        let _ = app_handle.emit(
            "dep:install-complete",
            serde_json::json!({ "dep_id": dep_id, "version": version }),
        );

        tracing::info!("Python dependency '{}' installed successfully", dep_id);
        Ok(())
    }

    // ==================== Internal ====================

    fn update_registry(&self, dep_id: &str, state: InstalledDepState) {
        let mut reg = self.registry.write_safe();

        // Preserve installed_at if not provided
        if state.installed_at.is_none() {
            if let Some(existing) = reg.get(dep_id) {
                if existing.installed_at.is_some() {
                    let mut new_state = state;
                    new_state.installed_at = existing.installed_at.clone();
                    reg.set(dep_id, new_state);
                    let _ = reg.save(&self.deps_dir);
                    return;
                }
            }
        }

        reg.set(dep_id, state);
        let _ = reg.save(&self.deps_dir);
    }

    fn set_error(&self, dep_id: &str, error: &str) {
        self.update_registry(
            dep_id,
            InstalledDepState {
                status: DepStatus::Error,
                version: None,
                installed_at: None,
                path: None,
                error_message: Some(error.to_string()),
            },
        );
    }

    /// Detect if a dependency is available via config override, bin dir, or system PATH
    fn detect_system(
        &self,
        def: &registry::DependencyDef,
        custom_path: &Option<String>,
        bin_dir: &Path,
    ) -> SystemDetection {
        // For runtime deps, system detection works differently
        if def.dep_type == registry::DepType::Runtime {
            return SystemDetection::default();
        }

        // If custom path is set and exists, it's system-available
        if let Some(ref cp) = custom_path {
            let p = std::path::Path::new(cp);
            if p.exists() {
                let version = def.version_check.as_ref().and_then(|vc| {
                    // Try to detect version from the custom path's directory
                    if let Some(dir) = p.parent() {
                        checker::detect_version(dir, vc)
                    } else {
                        None
                    }
                });
                return SystemDetection {
                    available: true,
                    path: Some(cp.clone()),
                    version,
                };
            }
        }

        // Check the first binary in def.binaries via bin_dir and PATH
        if let Some(&first_binary) = def.binaries.first() {
            if let Some(found) = ffmpeg::detect_binary(first_binary, bin_dir) {
                // Try to get version
                let version = def.version_check.as_ref().and_then(|vc| {
                    let found_path = std::path::Path::new(&found);
                    if let Some(dir) = found_path.parent() {
                        checker::detect_version(dir, vc)
                    } else {
                        None
                    }
                });
                return SystemDetection {
                    available: true,
                    path: Some(found),
                    version,
                };
            }
        }

        SystemDetection::default()
    }
}

// ==================== Config Overrides ====================

/// Collects custom paths from config.toml for dependency resolution
pub struct ConfigOverrides {
    overrides: std::collections::HashMap<String, String>,
}

impl ConfigOverrides {
    pub fn new() -> Self {
        Self {
            overrides: std::collections::HashMap::new(),
        }
    }

    pub fn add(&mut self, dep_id: &str, path: String) {
        if !path.is_empty() {
            self.overrides.insert(dep_id.to_string(), path);
        }
    }

    pub fn get(&self, dep_id: &str) -> Option<String> {
        self.overrides.get(dep_id).cloned()
    }
}

/// Build ConfigOverrides from AppConfig
pub fn config_overrides_from_app_config(config: &crate::config::AppConfig) -> ConfigOverrides {
    let mut overrides = ConfigOverrides::new();

    // FFmpeg: if either path is set, use the directory containing it
    if !config.ffmpeg.ffmpeg_path.is_empty() {
        overrides.add("ffmpeg", config.ffmpeg.ffmpeg_path.clone());
    }
    if !config.tools.danmaku_factory_path.is_empty() {
        overrides.add("danmaku-factory", config.tools.danmaku_factory_path.clone());
    }

    overrides
}

// ==================== Helpers ====================

/// Extract filename from URL
fn url_to_filename(url: &str) -> String {
    url.rsplit('/').next().unwrap_or("download.zip").to_string()
}

/// Get current timestamp as ISO string (without chrono crate)
fn chrono_now() -> String {
    // Use std::time for a simple timestamp
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}s", duration.as_secs())
}
