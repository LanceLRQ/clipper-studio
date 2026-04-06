use std::sync::Arc;

use serde_json::Value;

use super::error::PluginError;
use super::manifest::PluginManifest;
use super::transport::PluginTransport;

/// Plugin instance trait - main program calls plugins through this trait
#[async_trait::async_trait]
pub trait PluginInstance: Send + Sync {
    /// Get the plugin manifest
    fn manifest(&self) -> &PluginManifest;

    /// Initialize the plugin (called in main program context)
    async fn initialize(&self) -> Result<(), PluginError>;

    /// Shutdown the plugin
    async fn shutdown(&self) -> Result<(), PluginError>;

    /// Handle a request from the main program
    async fn handle_request(&self, action: &str, payload: Value) -> Result<Value, PluginError>;
}

/// Plugin builder trait - used for compile-time registration
pub trait PluginBuilder: Send + Sync {
    /// Unique plugin ID
    fn id(&self) -> &'static str;

    /// Build a plugin instance
    fn build(&self) -> Result<Box<dyn PluginInstance>, PluginError>;

    /// Get the plugin manifest (for listing purposes)
    fn manifest(&self) -> PluginManifest {
        self.build().unwrap().manifest().clone()
    }
}

/// Builtin transport - directly calls the plugin instance
pub struct BuiltinTransport {
    instance: Arc<Box<dyn PluginInstance>>,
}

impl BuiltinTransport {
    pub fn new(instance: Box<dyn PluginInstance>) -> Self {
        Self {
            instance: Arc::new(instance),
        }
    }
}

#[async_trait::async_trait]
impl PluginTransport for BuiltinTransport {
    async fn request(&self, action: &str, payload: Value) -> Result<Value, String> {
        self.instance
            .handle_request(action, payload)
            .await
            .map_err(|e| e.to_string())
    }

    async fn health(&self) -> Result<bool, String> {
        // Builtin plugins are always "healthy" if they can be loaded
        Ok(true)
    }

    async fn shutdown(&self) -> Result<(), String> {
        self.instance
            .shutdown()
            .await
            .map_err(|e| e.to_string())
    }
}
