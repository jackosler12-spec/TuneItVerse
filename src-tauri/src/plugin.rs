// plugin.rs — Foundation for TuneItVerse plugin and driver SDK (v3.48.0)
// Allows community to extend ECU support without forking core.
// Currently a stub: load definition packs already exist; this prepares for dynamic drivers.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub ecu_families: Vec<String>,
    pub description: String,
}

pub struct PluginManager {
    plugins: HashMap<String, PluginManifest>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    pub fn register(&mut self, manifest: PluginManifest) {
        self.plugins.insert(manifest.name.clone(), manifest);
    }

    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.values().collect()
    }
}

// Future: load from .dll or .so via libloading, or JSON manifests.
// Keep fail-closed: plugins cannot enable write without explicit capability flags.
