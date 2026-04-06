use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

use super::manifest::{
    load_manifest, PluginManifest, PluginMeta, PluginStatus, PluginType, Transport,
};
use super::service::ServiceManager;
use super::transport::{HttpTransport, PluginTransport, StdioTransport};

/// Supported API version range
const MIN_API_VERSION: u32 = 1;
const MAX_API_VERSION: u32 = 1;

/// Result of scanning a single plugin directory
#[derive(Debug, Clone, Serialize)]
pub struct PluginScanResult {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: String,
    pub status: String,
    /// Error message if scan failed
    pub error: Option<String>,
}

/// Central plugin manager: discovery, loading, lifecycle, communication
pub struct PluginManager {
    /// All discovered plugins (id → meta)
    plugins: RwLock<HashMap<String, PluginMeta>>,
    /// Active transports (id → transport)
    transports: RwLock<HashMap<String, Arc<Box<dyn PluginTransport>>>>,
    /// Service managers for managed plugins (id → service)
    services: RwLock<HashMap<String, Arc<ServiceManager>>>,
    /// Plugin search directory
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            transports: RwLock::new(HashMap::new()),
            services: RwLock::new(HashMap::new()),
            plugin_dir,
        }
    }

    /// Scan plugin directory for all plugin.json manifests
    pub async fn scan(&self) -> Vec<PluginScanResult> {
        let mut results = Vec::new();

        if !self.plugin_dir.exists() {
            tracing::debug!(
                "Plugin directory does not exist: {}",
                self.plugin_dir.display()
            );
            return results;
        }

        let entries = match std::fs::read_dir(&self.plugin_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read plugin directory: {}", e);
                return results;
            }
        };

        let mut plugins = self.plugins.write().await;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            match load_manifest(&path) {
                Ok(manifest) => {
                    let id = manifest.id.clone();

                    // Check API version compatibility
                    let status = if manifest.api_version < MIN_API_VERSION
                        || manifest.api_version > MAX_API_VERSION
                    {
                        results.push(PluginScanResult {
                            id: id.clone(),
                            name: manifest.name.clone(),
                            version: manifest.version.clone(),
                            plugin_type: format!("{:?}", manifest.plugin_type),
                            status: "incompatible".to_string(),
                            error: Some(format!(
                                "Unsupported API version {} (supported: {}-{})",
                                manifest.api_version, MIN_API_VERSION, MAX_API_VERSION
                            )),
                        });
                        PluginStatus::Error
                    } else {
                        results.push(PluginScanResult {
                            id: id.clone(),
                            name: manifest.name.clone(),
                            version: manifest.version.clone(),
                            plugin_type: format!("{:?}", manifest.plugin_type),
                            status: "discovered".to_string(),
                            error: None,
                        });
                        PluginStatus::Discovered
                    };

                    plugins.insert(
                        id,
                        PluginMeta {
                            manifest,
                            dir: path,
                            status,
                        },
                    );
                }
                Err(e) => {
                    let dir_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    tracing::debug!("Skipping {}: {}", dir_name, e);
                }
            }
        }

        tracing::info!("Plugin scan: {} plugins found", results.len());
        results
    }

    /// Load a plugin: verify dependencies, create transport
    pub async fn load(&self, plugin_id: &str) -> Result<(), String> {
        let mut plugins = self.plugins.write().await;
        let meta = plugins
            .get(plugin_id)
            .ok_or(format!("Plugin '{}' not found", plugin_id))?
            .clone();

        // Check dependencies
        self.check_dependencies(&meta.manifest, &plugins)?;

        // Check conflicts
        self.check_conflicts(&meta.manifest, &plugins)?;

        // Create transport
        let transport: Box<dyn PluginTransport> = match meta.manifest.transport {
            Transport::Http => {
                let port = meta.manifest.port.unwrap_or(8080);
                Box::new(HttpTransport::new(
                    &format!("http://127.0.0.1:{}", port),
                    meta.manifest.health_endpoint.as_deref(),
                ))
            }
            Transport::Stdio => {
                let exec_path = meta
                    .manifest
                    .executable
                    .as_ref()
                    .and_then(|e| e.current())
                    .ok_or("No executable defined for current platform")?;

                let full_path = meta.dir.join(exec_path);
                Box::new(StdioTransport::new(full_path, meta.dir.clone()))
            }
        };

        self.transports
            .write()
            .await
            .insert(plugin_id.to_string(), Arc::new(transport));

        // Update status
        if let Some(meta) = plugins.get_mut(plugin_id) {
            meta.status = PluginStatus::Loaded;
        }

        tracing::info!("Plugin loaded: {}", plugin_id);
        Ok(())
    }

    /// Unload a plugin: stop service if running, remove transport
    pub async fn unload(&self, plugin_id: &str) -> Result<(), String> {
        // Stop service if running
        self.stop_service(plugin_id).await.ok();

        // Remove transport
        self.transports.write().await.remove(plugin_id);

        // Update status
        if let Some(meta) = self.plugins.write().await.get_mut(plugin_id) {
            meta.status = PluginStatus::Discovered;
        }

        tracing::info!("Plugin unloaded: {}", plugin_id);
        Ok(())
    }

    /// Start a managed service plugin
    pub async fn start_service(&self, plugin_id: &str) -> Result<(), String> {
        let plugins = self.plugins.read().await;
        let meta = plugins
            .get(plugin_id)
            .ok_or(format!("Plugin '{}' not found", plugin_id))?;

        if !meta.manifest.managed {
            return Err("Not a managed service plugin".to_string());
        }

        let startup = meta
            .manifest
            .startup
            .as_ref()
            .ok_or("No startup command defined")?
            .clone();

        let port = meta.manifest.port.unwrap_or(8080);
        let health_ep = meta
            .manifest
            .health_endpoint
            .clone()
            .unwrap_or("/health".to_string());
        let dir = meta.dir.clone();

        drop(plugins); // Release read lock

        let svc = Arc::new(ServiceManager::new(plugin_id, port, &health_ep));
        svc.start(&startup, &dir).await?;

        self.services
            .write()
            .await
            .insert(plugin_id.to_string(), svc);

        // Update status
        if let Some(meta) = self.plugins.write().await.get_mut(plugin_id) {
            meta.status = PluginStatus::Running;
        }

        Ok(())
    }

    /// Stop a managed service plugin
    pub async fn stop_service(&self, plugin_id: &str) -> Result<(), String> {
        if let Some(svc) = self.services.write().await.remove(plugin_id) {
            svc.stop().await?;
        }

        // Update status back to Loaded
        if let Some(meta) = self.plugins.write().await.get_mut(plugin_id) {
            if meta.status == PluginStatus::Running {
                meta.status = PluginStatus::Loaded;
            }
        }

        Ok(())
    }

    /// Call a plugin via its transport
    pub async fn call(
        &self,
        plugin_id: &str,
        action: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let transports = self.transports.read().await;
        let transport = transports
            .get(plugin_id)
            .ok_or(format!("Plugin '{}' not loaded", plugin_id))?;

        transport.request(action, payload).await
    }

    /// Get a transport by plugin_id (for custom HTTP calls)
    pub async fn get_transport(
        &self,
        plugin_id: &str,
    ) -> Option<Arc<Box<dyn PluginTransport>>> {
        self.transports.read().await.get(plugin_id).cloned()
    }

    /// List all discovered plugins
    pub async fn list(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        let services = self.services.read().await;

        plugins
            .values()
            .map(|meta| {
                let is_running = services
                    .get(&meta.manifest.id)
                    .map(|s| s.is_running())
                    .unwrap_or(false);

                PluginInfo {
                    id: meta.manifest.id.clone(),
                    name: meta.manifest.name.clone(),
                    version: meta.manifest.version.clone(),
                    plugin_type: format!("{:?}", meta.manifest.plugin_type),
                    transport: format!("{:?}", meta.manifest.transport),
                    managed: meta.manifest.managed,
                    status: if is_running {
                        "running".to_string()
                    } else {
                        format!("{:?}", meta.status).to_lowercase()
                    },
                    description: meta.manifest.description.clone(),
                    has_config: !meta.manifest.config_schema.is_empty(),
                }
            })
            .collect()
    }

    /// List plugins by type
    pub async fn list_by_type(&self, plugin_type: &PluginType) -> Vec<PluginInfo> {
        let all = self.list().await;
        let type_str = format!("{:?}", plugin_type);
        all.into_iter()
            .filter(|p| p.plugin_type == type_str)
            .collect()
    }

    /// Shutdown all services on app exit
    pub async fn shutdown_all(&self) {
        let mut services = self.services.write().await;
        for (id, svc) in services.drain() {
            tracing::info!("Shutting down plugin service: {}", id);
            let _ = svc.stop().await;
        }
    }

    // ====== Internal helpers ======

    fn check_dependencies(
        &self,
        manifest: &PluginManifest,
        plugins: &HashMap<String, PluginMeta>,
    ) -> Result<(), String> {
        for dep in &manifest.dependencies {
            if !plugins.contains_key(&dep.id) {
                return Err(format!(
                    "Missing dependency: {} (required by {})",
                    dep.id, manifest.id
                ));
            }
            // TODO: semver version range check
        }
        Ok(())
    }

    fn check_conflicts(
        &self,
        manifest: &PluginManifest,
        plugins: &HashMap<String, PluginMeta>,
    ) -> Result<(), String> {
        for conflict in &manifest.conflicts {
            if let Some(other) = plugins.get(&conflict.id) {
                if other.status == PluginStatus::Loaded || other.status == PluginStatus::Running {
                    let reason = conflict
                        .reason
                        .as_deref()
                        .unwrap_or("conflict declared in manifest");
                    return Err(format!(
                        "Plugin conflict with '{}': {}",
                        conflict.id, reason
                    ));
                }
            }
        }

        // Singleton check
        if manifest.singleton {
            for (id, other) in plugins {
                if id != &manifest.id
                    && other.manifest.plugin_type == manifest.plugin_type
                    && (other.status == PluginStatus::Loaded
                        || other.status == PluginStatus::Running)
                {
                    return Err(format!(
                        "Singleton conflict: {} (type {:?}) already active",
                        id, manifest.plugin_type
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Serializable plugin info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: String,
    pub transport: String,
    pub managed: bool,
    pub status: String,
    pub description: Option<String>,
    pub has_config: bool,
}
