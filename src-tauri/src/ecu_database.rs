//! ecu_database.rs — Backend database of known ECUs for TuneItVerse
//!
//! Loads embedded JSON definitions for P01_0411, EDC16C41, GM P59, MED17_COMMON (and future).
//! Provides lookup by family / OS ID so the app can auto-configure checksum,
//! security, maps, and protocol on ECU connect or bin load.
//!
//! Now with 4 families in v0.4.0. Scalable: add new JSON + const include + push.

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ChecksumInfo {
    pub r#type: String,
    pub description: String,
    pub reference_files: Vec<String>,
    pub routine: Option<String>,
    pub notes: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct SecurityInfo {
    pub r#type: String,
    pub reference: String,
    pub levels: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct MapsXdfInfo {
    pub has_xdf: bool,
    pub xdf_file: Option<String>,
    pub description: String,
    pub kernel_for_flash: Option<String>,
    pub additional_defs: Option<Vec<String>>,
    pub eeprom: Option<String>,
    pub refined_map_addrs: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct EcuDbEntry {
    pub ecu_family: String,
    pub display_name: String,
    pub part_numbers_or_os_ids: Vec<String>,
    pub vehicles: Vec<String>,
    pub hardware: String,
    pub protocol: String,
    pub bin_size_bytes: u32,
    pub flash_memory_map: Option<serde_json::Value>,
    pub checksum: ChecksumInfo,
    pub security_access: SecurityInfo,
    pub maps_and_xdf: MapsXdfInfo,
    pub communication_kernels: Option<Vec<String>>,
    pub notes: String,
}

/// Embedded JSONs (reference/ folder at repo root) - v0.4.0 expanded
const P01_JSON: &str = include_str!("../../reference/ecu_database/p01_0411.json");
const EDC16_JSON: &str = include_str!("../../reference/ecu_database/edc16c41_nissan_patrol.json");
const P59_JSON: &str = include_str!("../../reference/ecu_database/gm_p59.json");
const MED17_JSON: &str = include_str!("../../reference/ecu_database/med17_common.json");

/// Load all known ECU entries (embedded for distributable binary)
pub fn load_ecu_database() -> Vec<EcuDbEntry> {
    let mut db = Vec::new();

    if let Ok(entry) = serde_json::from_str::<EcuDbEntry>(P01_JSON) {
        db.push(entry);
    }
    if let Ok(entry) = serde_json::from_str::<EcuDbEntry>(EDC16_JSON) {
        db.push(entry);
    }
    if let Ok(entry) = serde_json::from_str::<EcuDbEntry>(P59_JSON) {
        db.push(entry);
    }
    if let Ok(entry) = serde_json::from_str::<EcuDbEntry>(MED17_JSON) {
        db.push(entry);
    }
    // Future: load more JSONs or from directory scan (build script)
    db
}

/// Lookup by ECU family key (e.g. "P01_0411" or "EDC16C41" or "MED17_COMMON")
pub fn get_ecu_by_family(family: &str) -> Option<EcuDbEntry> {
    load_ecu_database()
        .into_iter()
        .find(|e| e.ecu_family.eq_ignore_ascii_case(family))
}

/// Simple lookup by OS ID or part number (contains match)
pub fn get_ecu_by_os_id(os_id: &str) -> Option<EcuDbEntry> {
    let os = os_id.to_ascii_uppercase();
    load_ecu_database().into_iter().find(|e| {
        e.part_numbers_or_os_ids.iter().any(|id| id.to_ascii_uppercase().contains(&os))
            || e.display_name.to_ascii_uppercase().contains(&os)
    })
}

/// Return list of supported families (for frontend dropdown etc.)
pub fn list_supported_ecu_families() -> Vec<String> {
    load_ecu_database().into_iter().map(|e| e.ecu_family).collect()
}
