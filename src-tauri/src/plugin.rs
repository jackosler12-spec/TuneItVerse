//! plugin.rs — Driver plugin SDK foundation for TuneItVerse
//!
//! Allows registering driver plugins for new protocols and ECU families without core recompile.
//! Skeleton for future dynamic .dll/.so loading via libloading. Built-in example included.
//! Enables community growth of coverage while keeping fail-closed safety in core.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub protocols: Vec<String>,
    pub families: Vec<String>,
}

pub trait DriverPlugin: Send + Sync {
    fn info(&self) -> PluginInfo;
    fn identify(&self, data: &[u8]) -> Option<String>;
    // Extend with read, write, checksum, seed_key as plugins mature. Core remains fail-closed.
}

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn DriverPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn DriverPlugin>) {
        let info = plugin.info();
        self.plugins.insert(info.name.clone(), plugin);
    }

    pub fn list(&self) -> Vec<PluginInfo> {
        self.plugins.values().map(|p| p.info()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&dyn DriverPlugin> {
        self.plugins.get(name).map(|b| b.as_ref())
    }
}

/// Example built-in plugin demonstrating the interface.
pub struct ExamplePlugin;

impl DriverPlugin for ExamplePlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "example_can".into(),
            version: "0.1.0".into(),
            protocols: vec!["CAN".into(), "ISO-TP".into()],
            families: vec!["EXAMPLE_FAMILY".into()],
        }
    }

    fn identify(&self, _data: &[u8]) -> Option<String> {
        None // No real identification yet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_works() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(ExamplePlugin));
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "example_can");
    }
}
