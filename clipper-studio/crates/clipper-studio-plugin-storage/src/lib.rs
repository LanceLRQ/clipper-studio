//! Builtin storage-provider plugin for SMB/CIFS network mounts
//!
//! Mounts remote SMB/CIFS shares to local paths via platform-specific commands:
//! - macOS: `mount_smbfs`
//! - Linux: `mount.cifs` (may require sudo/fstab entry)
//! - Windows: `net use`

use async_trait::async_trait;
use clipper_studio_plugin_core::{
    PluginError, PluginInstance, PluginManifest, PluginType, Transport,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

mod mount;

use mount::{MountBackend, MountInfo, MountParams};

/// Storage provider plugin instance
pub struct StorageProviderPlugin {
    manifest: PluginManifest,
    /// Active mounts tracked by this plugin
    active_mounts: RwLock<Vec<MountInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MountRequest {
    /// SMB server address (e.g. "192.168.1.100")
    server: String,
    /// Share name (e.g. "recordings")
    share: String,
    /// Username for authentication
    #[serde(default)]
    username: String,
    /// Password for authentication
    #[serde(default)]
    password: String,
    /// Local mount point path (optional, auto-generated if empty)
    #[serde(default)]
    mount_point: String,
}

#[async_trait]
impl PluginInstance for StorageProviderPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn initialize(&self) -> Result<(), PluginError> {
        tracing::info!("Initializing StorageProvider plugin");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PluginError> {
        tracing::info!("Shutting down StorageProvider plugin, unmounting all...");
        let mounts = self.active_mounts.read().await.clone();
        for info in &mounts {
            if let Err(e) = MountBackend::unmount(&info.mount_point).await {
                tracing::warn!("Failed to unmount {}: {}", info.mount_point, e);
            }
        }
        self.active_mounts.write().await.clear();
        tracing::info!("StorageProvider shutdown complete");
        Ok(())
    }

    async fn handle_request(
        &self,
        action: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        match action {
            "mount" => self.handle_mount(payload).await,
            "unmount" => self.handle_unmount(payload).await,
            "list_mounts" => self.handle_list_mounts().await,
            "check" => self.handle_check().await,
            "detect_mounts" => self.handle_detect_mounts(payload).await,
            _ => Err(PluginError::UnsupportedAction(action.to_string())),
        }
    }
}

impl StorageProviderPlugin {
    pub fn new() -> Self {
        let manifest = PluginManifest {
            id: "builtin.storage.smb".to_string(),
            name: "SMB/CIFS 网络存储".to_string(),
            plugin_type: PluginType::StorageProvider,
            version: "1.0.0".to_string(),
            api_version: 1,
            transport: Transport::Builtin,
            managed: false,
            singleton: true,
            startup: None,
            executable: None,
            health_endpoint: None,
            port: None,
            config_schema: std::collections::HashMap::new(),
            dependencies: vec![],
            conflicts: vec![],
            description: Some(
                "挂载 SMB/CIFS 网络共享（NAS）到本地目录，作为工作区使用".to_string(),
            ),
            frontend: None,
        };

        Self {
            manifest,
            active_mounts: RwLock::new(Vec::new()),
        }
    }

    /// Handle mount action
    async fn handle_mount(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let req: MountRequest = serde_json::from_value(payload)
            .map_err(|e| PluginError::InvalidPayload(format!("Invalid mount request: {}", e)))?;

        if req.server.is_empty() || req.share.is_empty() {
            return Err(PluginError::InvalidPayload(
                "server and share are required".to_string(),
            ));
        }

        let params = MountParams {
            server: req.server.clone(),
            share: req.share.clone(),
            username: if req.username.is_empty() {
                None
            } else {
                Some(req.username.clone())
            },
            password: if req.password.is_empty() {
                None
            } else {
                Some(req.password.clone())
            },
            mount_point: if req.mount_point.is_empty() {
                None
            } else {
                Some(req.mount_point.clone())
            },
        };

        let info = MountBackend::mount(params)
            .await
            .map_err(|e| PluginError::Transport(format!("Mount failed: {}", e)))?;

        tracing::info!(
            "SMB share mounted: //{}/{} -> {}",
            info.server,
            info.share,
            info.mount_point
        );

        // Track this mount
        self.active_mounts.write().await.push(info.clone());

        Ok(serde_json::to_value(&info).unwrap_or_default())
    }

    /// Handle unmount action
    async fn handle_unmount(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let mount_point = payload
            .get("mount_point")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PluginError::InvalidPayload("mount_point is required".to_string()))?
            .to_string();

        MountBackend::unmount(&mount_point)
            .await
            .map_err(|e| PluginError::Transport(format!("Unmount failed: {}", e)))?;

        // Remove from tracked mounts
        let mut mounts = self.active_mounts.write().await;
        mounts.retain(|m| m.mount_point != mount_point);

        tracing::info!("SMB share unmounted: {}", mount_point);

        Ok(serde_json::json!({"ok": true}))
    }

    /// List active mounts (with live validation)
    async fn handle_list_mounts(&self) -> Result<serde_json::Value, PluginError> {
        // Validate each tracked mount is still alive
        let current = self.active_mounts.read().await.clone();
        let mut alive = Vec::new();
        for info in &current {
            if MountBackend::is_mount_point(&info.mount_point).await {
                alive.push(info.clone());
            }
        }
        // Update tracking to remove stale entries
        if alive.len() != current.len() {
            *self.active_mounts.write().await = alive.clone();
        }
        Ok(serde_json::to_value(&alive).unwrap_or_default())
    }

    /// Detect which mount points from a given list are still active at OS level.
    /// Payload: { "candidates": [{ "server": "...", "share": "...", "mount_point": "..." }, ...] }
    /// Returns the subset that are currently mounted, and syncs them into active_mounts.
    async fn handle_detect_mounts(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let candidates: Vec<MountInfo> = payload
            .get("candidates")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let mut detected = Vec::new();
        for c in &candidates {
            if !c.mount_point.is_empty() && MountBackend::is_mount_point(&c.mount_point).await {
                detected.push(c.clone());
            }
        }

        // Merge detected into active_mounts (avoid duplicates)
        let mut mounts = self.active_mounts.write().await;
        for d in &detected {
            if !mounts.iter().any(|m| m.mount_point == d.mount_point) {
                mounts.push(d.clone());
            }
        }

        Ok(serde_json::to_value(&detected).unwrap_or_default())
    }

    /// Check platform support
    async fn handle_check(&self) -> Result<serde_json::Value, PluginError> {
        let supported = MountBackend::is_supported();
        let platform = std::env::consts::OS;
        Ok(serde_json::json!({
            "supported": supported,
            "platform": platform,
        }))
    }
}

impl Default for StorageProviderPlugin {
    fn default() -> Self {
        Self::new()
    }
}

// ===== PluginBuilder =====

pub struct StorageProviderPluginBuilder;

impl StorageProviderPluginBuilder {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StorageProviderPluginBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl clipper_studio_plugin_core::PluginBuilder for StorageProviderPluginBuilder {
    fn id(&self) -> &'static str {
        "builtin.storage.smb"
    }

    fn build(&self) -> Result<Box<dyn PluginInstance>, PluginError> {
        Ok(Box::new(StorageProviderPlugin::new()))
    }
}
