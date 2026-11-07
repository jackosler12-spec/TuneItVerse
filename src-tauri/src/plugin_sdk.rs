//! plugin_sdk.rs — Skeleton for driver / definition plugin extensibility.
//!
//! Community and third-party drivers can implement this trait to add
//! protocol adapters, checksum routines, or seed/key tables without
//! forking the core. Load via libloading at runtime (Windows DLL / .so).
//!
//! Status: experimental skeleton. No plugins ship in this tree.
//! Hardware paths remain untested. Write flags stay fail-closed.

use std::any::Any;

/// Capability a plugin can provide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCapability {
    ProtocolAdapter,
    ChecksumCorrector,
    SeedKeyAlgo,
    DefinitionPack,
    LiveProbe,
}

/// Result of a plugin operation.
#[derive(Debug)]
pub enum PluginResult {
    Ok(String),
    Refused(String),
    Error(String),
}

/// Core trait every plugin must implement.
pub trait TuneItPlugin: Send + Sync {
    /// Unique ID, e.g. "jrtuners.edc17_probe"
    fn id(&self) -> &str;
    /// Human name
    fn name(&self) -> &str;
    /// Version string
    fn version(&self) -> &str;
    /// What this plugin can do
    fn capabilities(&self) -> Vec<PluginCapability>;
    /// Optional: handle a named action with JSON args. Fail-closed by default.
    fn handle(&self, action: &str, _args: &serde_json::Value) -> PluginResult {
        PluginResult::Refused(format!("plugin {} does not implement action {}", self.id(), action))
    }
    /// Downcast helper
    fn as_any(&self) -> &dyn Any;
}

/// Registry of loaded plugins (in-memory for now).
pub struct PluginRegistry {
    plugins: Vec<Box<dyn TuneItPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }
    pub fn register(&mut self, p: Box<dyn TuneItPlugin>) {
        self.plugins.push(p);
    }
    pub fn list(&self) -> Vec<(&str, &str, Vec<PluginCapability>)> {
        self.plugins.iter().map(|p| (p.id(), p.name(), p.capabilities())).collect()
    }
    pub fn find_by_id(&self, id: &str) -> Option<&dyn TuneItPlugin> {
        self.plugins.iter().find(|p| p.id() == id).map(|p| p.as_ref())
    }
}

/// Example no-op plugin for tests.
pub struct NullPlugin;

impl TuneItPlugin for NullPlugin {
    fn id(&self) -> &str { "null.example" }
    fn name(&self) -> &str { "Null Example Plugin" }
    fn version(&self) -> &str { "0.1.0" }
    fn capabilities(&self) -> Vec<PluginCapability> { vec![] }
    fn as_any(&self) -> &dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_registers_and_lists() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(NullPlugin));
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "null.example");
    }
    #[test]
    fn handle_refuses_unknown() {
        let p = NullPlugin;
        match p.handle("flash", &serde_json::json!({})) {
            PluginResult::Refused(_) => {}
            _ => panic!("must refuse"),
        }
    }
}
