use crate::utils::locks::RwLockExt;
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
        self.builders
            .insert(builder.id().to_string(), Arc::new(builder));
    }

    /// Get all registered builtin plugin manifests (for listing)
    pub fn list_builtin(&self) -> Vec<PluginManifest> {
        self.builders.values().map(|b| b.manifest()).collect()
    }

    /// Get all registered builtin plugins with their current status
    pub fn list_builtin_with_status(
        &self,
        enabled_ids: &std::collections::HashSet<String>,
    ) -> Vec<BuiltinPluginInfo> {
        let loaded: std::collections::HashSet<String> =
            self.instances.read().unwrap().keys().cloned().collect();

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
    pub async fn load_builtin(
        &self,
        plugin_id: &str,
    ) -> Result<Arc<Box<dyn PluginTransport>>, String> {
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
            let mut guard = self.instances.write_safe();
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
        self.instances.read_safe().get(plugin_id).cloned()
    }

    /// Shutdown all loaded builtin plugins (called on app exit)
    pub async fn shutdown_all(&self) {
        let instances: Vec<(String, Arc<Box<dyn PluginTransport>>)> = {
            let mut guard = self.instances.write_safe();
            guard.drain().collect()
        };
        for (id, trans) in instances {
            tracing::info!("Shutting down builtin plugin: {}", id);
            match tokio::time::timeout(std::time::Duration::from_secs(5), trans.shutdown()).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!("Failed to shutdown builtin plugin {}: {}", id, e),
                Err(_) => tracing::warn!("Timeout shutting down builtin plugin {} (5s)", id),
            }
        }
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
            status: if is_loaded {
                "loaded".to_string()
            } else {
                "discovered".to_string()
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use clipper_studio_plugin_core::{
        PluginError, PluginInstance, PluginManifest, PluginType, Transport,
    };
    use serde_json::Value;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;

    fn make_manifest(id: &str, name: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: name.to_string(),
            plugin_type: PluginType::StorageProvider,
            version: "0.1.0".to_string(),
            api_version: 1,
            transport: Transport::Builtin,
            managed: false,
            singleton: false,
            startup: None,
            executable: None,
            health_endpoint: None,
            port: None,
            config_schema: HashMap::new(),
            dependencies: vec![],
            conflicts: vec![],
            description: Some("test".to_string()),
            frontend: None,
        }
    }

    struct MockInstance {
        manifest: PluginManifest,
        shutdown_counter: StdArc<AtomicUsize>,
    }

    #[async_trait]
    impl PluginInstance for MockInstance {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
        async fn initialize(&self) -> Result<(), PluginError> {
            Ok(())
        }
        async fn shutdown(&self) -> Result<(), PluginError> {
            self.shutdown_counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn handle_request(&self, action: &str, _: Value) -> Result<Value, PluginError> {
            Ok(Value::String(format!("ack:{}", action)))
        }
    }

    struct MockBuilder {
        id: &'static str,
        name: &'static str,
        shutdown_counter: StdArc<AtomicUsize>,
    }

    impl MockBuilder {
        fn new(id: &'static str, name: &'static str) -> (Self, StdArc<AtomicUsize>) {
            let counter = StdArc::new(AtomicUsize::new(0));
            (
                Self {
                    id,
                    name,
                    shutdown_counter: counter.clone(),
                },
                counter,
            )
        }
    }

    impl PluginBuilder for MockBuilder {
        fn id(&self) -> &'static str {
            self.id
        }
        fn build(&self) -> Result<Box<dyn PluginInstance>, PluginError> {
            Ok(Box::new(MockInstance {
                manifest: make_manifest(self.id, self.name),
                shutdown_counter: self.shutdown_counter.clone(),
            }))
        }
    }

    /// 故意 build 失败的 builder，验证错误传播
    struct FailingBuilder;
    impl PluginBuilder for FailingBuilder {
        fn id(&self) -> &'static str {
            "fail.builder"
        }
        fn build(&self) -> Result<Box<dyn PluginInstance>, PluginError> {
            Err(PluginError::InvalidPayload("boom".to_string()))
        }
    }

    #[test]
    fn test_new_registry_is_empty() {
        let reg = PluginRegistry::new();
        assert!(reg.list_builtin().is_empty());
        assert!(!reg.is_builtin("anything"));
    }

    #[test]
    fn test_default_equivalent_to_new() {
        let reg = PluginRegistry::default();
        assert!(reg.list_builtin().is_empty());
    }

    #[test]
    fn test_register_and_is_builtin() {
        let mut reg = PluginRegistry::new();
        let (b, _) = MockBuilder::new("p.a", "Plugin A");
        reg.register(b);
        assert!(reg.is_builtin("p.a"));
        assert!(!reg.is_builtin("p.b"));
    }

    #[test]
    fn test_register_overwrites_same_id() {
        let mut reg = PluginRegistry::new();
        let (b1, _) = MockBuilder::new("dup", "First");
        let (b2, _) = MockBuilder::new("dup", "Second");
        reg.register(b1);
        reg.register(b2);
        // 同 id 应覆盖：仅剩 1 条，且 name 为后者
        let manifests = reg.list_builtin();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].name, "Second");
    }

    #[test]
    fn test_list_builtin_returns_all_manifests() {
        let mut reg = PluginRegistry::new();
        let (a, _) = MockBuilder::new("a", "A");
        let (b, _) = MockBuilder::new("b", "B");
        reg.register(a);
        reg.register(b);
        let mut ids: Vec<String> = reg.list_builtin().into_iter().map(|m| m.id).collect();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn test_load_builtin_unknown_id_errors() {
        let reg = PluginRegistry::new();
        let result = reg.load_builtin("ghost").await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("应失败"),
        };
        assert!(err.contains("ghost"), "错误应包含 plugin id：{}", err);
    }

    #[tokio::test]
    async fn test_load_builtin_propagates_build_error() {
        let mut reg = PluginRegistry::new();
        reg.register(FailingBuilder);
        let result = reg.load_builtin("fail.builder").await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("应失败"),
        };
        assert!(err.contains("boom"), "build 错误应被传播：{}", err);
    }

    #[tokio::test]
    async fn test_load_then_get_transport() {
        let mut reg = PluginRegistry::new();
        let (b, _) = MockBuilder::new("loadable", "L");
        reg.register(b);

        assert!(reg.get_transport("loadable").is_none(), "未加载时应为 None");
        reg.load_builtin("loadable").await.expect("load");
        let trans = reg.get_transport("loadable").expect("已加载");

        // 通过 transport 调用 handle_request
        let result = trans
            .request("ping", Value::Null)
            .await
            .expect("request ok");
        assert_eq!(result, Value::String("ack:ping".to_string()));

        // health 对 builtin 永远返回 true
        assert!(trans.health().await.unwrap());
    }

    #[tokio::test]
    async fn test_unload_triggers_shutdown() {
        let mut reg = PluginRegistry::new();
        let (b, counter) = MockBuilder::new("to_unload", "U");
        reg.register(b);

        reg.load_builtin("to_unload").await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 0, "load 不触发 shutdown");

        reg.unload_builtin("to_unload").await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1, "shutdown 应被调用一次");
        assert!(reg.get_transport("to_unload").is_none(), "应已卸载");
    }

    #[tokio::test]
    async fn test_unload_unknown_is_noop() {
        // unload 不存在的插件不应报错
        let reg = PluginRegistry::new();
        reg.unload_builtin("nonexistent").await.expect("应静默通过");
    }

    #[tokio::test]
    async fn test_list_builtin_with_status_reflects_load_and_enable() {
        let mut reg = PluginRegistry::new();
        let (x, _) = MockBuilder::new("x", "X");
        let (y, _) = MockBuilder::new("y", "Y");
        reg.register(x);
        reg.register(y);
        reg.load_builtin("x").await.unwrap();

        let mut enabled = HashSet::new();
        enabled.insert("y".to_string());

        let infos = reg.list_builtin_with_status(&enabled);
        let info_x = infos.iter().find(|i| i.id == "x").expect("x");
        let info_y = infos.iter().find(|i| i.id == "y").expect("y");

        assert_eq!(info_x.status, "loaded", "x 已加载");
        assert!(!info_x.enabled, "x 未启用");
        assert_eq!(info_y.status, "discovered", "y 未加载");
        assert!(info_y.enabled, "y 已启用");
    }

    #[tokio::test]
    async fn test_shutdown_all_clears_instances() {
        let mut reg = PluginRegistry::new();
        let (s1, c1) = MockBuilder::new("s1", "S1");
        let (s2, c2) = MockBuilder::new("s2", "S2");
        reg.register(s1);
        reg.register(s2);
        reg.load_builtin("s1").await.unwrap();
        reg.load_builtin("s2").await.unwrap();

        reg.shutdown_all().await;

        assert_eq!(c1.load(Ordering::SeqCst), 1, "s1 应被 shutdown 一次");
        assert_eq!(c2.load(Ordering::SeqCst), 1, "s2 应被 shutdown 一次");
        assert!(reg.get_transport("s1").is_none());
        assert!(reg.get_transport("s2").is_none());
    }

    #[test]
    fn test_builtin_plugin_info_status_string_loaded() {
        let m = make_manifest("ok", "OK");
        let info = BuiltinPluginInfo::from(m.clone(), true, false);
        assert_eq!(info.status, "loaded");
        assert!(!info.enabled);
        assert_eq!(info.id, "ok");
        assert_eq!(info.transport, "Builtin");
    }

    #[test]
    fn test_builtin_plugin_info_status_string_discovered() {
        let m = make_manifest("ok", "OK");
        let info = BuiltinPluginInfo::from(m, false, true);
        assert_eq!(info.status, "discovered");
        assert!(info.enabled);
    }

    #[test]
    fn test_builtin_plugin_info_dir_always_none() {
        let m = make_manifest("any", "Any");
        let info = BuiltinPluginInfo::from(m, true, true);
        assert!(info.dir.is_none(), "builtin 插件 dir 永远为 None");
    }

    #[test]
    fn test_builtin_plugin_info_has_config_false_when_empty_schema() {
        let m = make_manifest("noschema", "No");
        let info = BuiltinPluginInfo::from(m, false, false);
        assert!(!info.has_config);
        assert!(info.config_schema.is_none());
    }

    #[test]
    fn test_builtin_plugin_info_has_config_true_when_schema_present() {
        let mut m = make_manifest("withschema", "With");
        m.config_schema
            .insert("api_url".to_string(), serde_json::json!({"type": "string"}));
        let info = BuiltinPluginInfo::from(m, false, false);
        assert!(info.has_config);
        assert!(info.config_schema.is_some());
    }
}
