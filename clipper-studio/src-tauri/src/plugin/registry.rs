use std::collections::HashMap;
use std::sync::Arc;

use clipper_studio_plugin_core::{
    BuiltinTransport, PluginBuilder, PluginManifest, PluginTransport,
};

/// Compile-time plugin registry for builtin plugins
pub struct PluginRegistry {
    builders: HashMap<String, Arc<dyn PluginBuilder>>,
    instances: std::sync::RwLock<HashMap<String, Arc<Box<dyn PluginTransport>>>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
            instances: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Register a builtin plugin builder
    pub fn register<T: PluginBuilder + 'static>(&mut self, builder: T) {
        self.builders.insert(builder.id().to_string(), Arc::new(builder));
    }

    /// Get all registered builtin plugin manifests (for listing)
    pub fn list_builtin(&self) -> Vec<PluginManifest> {
        self.builders.values().map(|b| b.manifest()).collect()
    }

    /// Get all registered builtin plugins with their current status
    pub fn list_builtin_with_status(&self, enabled_ids: &std::collections::HashSet<String>) -> Vec<BuiltinPluginInfo> {
        let loaded: std::collections::HashSet<String> = self
            .instances
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect();

        self.builders
            .values()
            .map(|b| {
                let manifest = b.manifest();
                let is_loaded = loaded.contains(manifest.id.as_str());
                let is_enabled = enabled_ids.contains(manifest.id.as_str());
                BuiltinPluginInfo::from(manifest.clone(), is_loaded, is_enabled)
            })
            .collect()
    }

    /// Check if a plugin ID is a builtin plugin
    pub fn is_builtin(&self, plugin_id: &str) -> bool {
        self.builders.contains_key(plugin_id)
    }

    /// Load a builtin plugin and create its transport
    pub async fn load_builtin(&self, plugin_id: &str) -> Result<Arc<Box<dyn PluginTransport>>, String> {
        let builder = self
            .builders
            .get(plugin_id)
            .ok_or(format!("Builtin plugin '{}' not found", plugin_id))?
            .clone();

        let instance = builder.build().map_err(|e| e.to_string())?;
        let transport: Box<dyn PluginTransport> = Box::new(BuiltinTransport::new(instance));

        let transport = Arc::new(transport);
        self.instances
            .write()
            .unwrap()
            .insert(plugin_id.to_string(), transport.clone());

        Ok(transport)
    }

    /// Unload a builtin plugin
    pub async fn unload_builtin(&self, plugin_id: &str) -> Result<(), String> {
        // Extract transport from lock before awaiting (lock must not be held across await)
        let transport: Option<Arc<Box<dyn PluginTransport>>> = {
            let mut guard = self.instances.write().unwrap();
            guard.remove(plugin_id)
        };
        // Now we can await without holding the lock
        if let Some(trans) = transport {
            trans.shutdown().await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Get a loaded builtin plugin's transport
    pub fn get_transport(&self, plugin_id: &str) -> Option<Arc<Box<dyn PluginTransport>>> {
        self.instances.read().unwrap().get(plugin_id).cloned()
    }
}

/// Serializable plugin info for frontend (used for builtin plugins)
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuiltinPluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: String,
    pub transport: String,
    pub managed: bool,
    pub status: String,
    pub description: Option<String>,
    pub has_config: bool,
    /// Whether the plugin is enabled (persisted, auto-loaded on startup)
    pub enabled: bool,
    pub config_schema: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend: Option<clipper_studio_plugin_core::PluginFrontend>,
    /// Plugin directory path (always None for builtin plugins)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

impl BuiltinPluginInfo {
    pub fn from(m: PluginManifest, is_loaded: bool, is_enabled: bool) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            plugin_type: format!("{:?}", m.plugin_type),
            transport: format!("{:?}", m.transport),
            managed: m.managed,
            status: if is_loaded { "loaded".to_string() } else { "discovered".to_string() },
            description: m.description.clone(),
            has_config: !m.config_schema.is_empty(),
            enabled: is_enabled,
            config_schema: if m.config_schema.is_empty() {
                None
            } else {
                Some(m.config_schema)
            },
            frontend: m.frontend.clone(),
            dir: None,
        }
    }
}
