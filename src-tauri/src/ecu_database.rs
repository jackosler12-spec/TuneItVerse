//! ecu_database.rs — Backend database of known ECUs for TuneItVerse
//!
//! Loads embedded JSON definitions for P01_0411, EDC16C41, GM P59, MED17_COMMON, EDC17_COMMON (and future).
//! Provides lookup by family / OS ID so the app can auto-configure checksum,
//! security, maps, and protocol on ECU connect or bin load.
//!
//! v2.3.0: Dynamic table defs from refined_map_addrs + size-aware fallback. Industry-leading scalable DB.

use serde::{Deserialize, Serialize};
use crate::xdf::TableDef;

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

/// Embedded JSONs (reference/ folder at repo root) - v2.3.0
const P01_JSON: &str = include_str!("../../reference/ecu_database/p01_0411.json");
const EDC16_JSON: &str = include_str!("../../reference/ecu_database/edc16c41_nissan_patrol.json");
const P59_JSON: &str = include_str!("../../reference/ecu_database/gm_p59.json");
const MED17_JSON: &str = include_str!("../../reference/ecu_database/med17_common.json");
const EDC17_JSON: &str = include_str!("../../reference/ecu_database/edc17_common.json");

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
    if let Ok(entry) = serde_json::from_str::<EcuDbEntry>(EDC17_JSON) {
        db.push(entry);
    }
    // Future: load more JSONs or from directory scan (build script)
    db
}

/// Lookup by ECU family key (e.g. "P01_0411" or "EDC16C41" or "EDC17_COMMON")
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

/// Find ECU entry matching bin size (primary fingerprint for auto-load)
pub fn get_ecu_by_bin_size(size: usize) -> Option<EcuDbEntry> {
    load_ecu_database()
        .into_iter()
        .find(|e| e.bin_size_bytes as usize == size)
}

/// Build TableDef list from refined_map_addrs (diesel families) or size defaults (P01).
/// This powers true DB-driven auto_load_tables_for_bin — industry-leading scalable maps.
pub fn get_tables_for_bin_size(size: usize) -> Vec<TableDef> {
    if let Some(entry) = get_ecu_by_bin_size(size) {
        if let Some(addrs) = entry.maps_and_xdf.refined_map_addrs {
            let mut tables = Vec::new();
            // Driver wish / torque
            if let Some(addr) = addrs.get("driver_wish").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "driver-wish".into(),
                    name: "Driver Wish (Torque)".into(),
                    description: format!("Driver requested torque — {}", entry.ecu_family),
                    rows: 16,
                    cols: 16,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "Nm".into(),
                    category: Some("Torque".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("injection_quantity").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "inj-quantity".into(),
                    name: "Injection Quantity".into(),
                    description: "IQ main map".into(),
                    rows: 16,
                    cols: 16,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.01".into(),
                    units: "mm3".into(),
                    category: Some("Fuel".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("boost_setpoint").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "boost-setpoint".into(),
                    name: "Boost Setpoint".into(),
                    description: "Target boost".into(),
                    rows: 12,
                    cols: 12,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "mbar".into(),
                    category: Some("Boost".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("rail_pressure").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "rail-pressure".into(),
                    name: "Rail Pressure".into(),
                    description: "Rail pressure setpoint".into(),
                    rows: 12,
                    cols: 12,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x".into(),
                    units: "bar".into(),
                    category: Some("Fuel".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("vgt_duty").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "vgt-duty".into(),
                    name: "VGT Duty".into(),
                    description: "Variable geometry turbo duty".into(),
                    rows: 12,
                    cols: 12,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "%".into(),
                    category: Some("Boost".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("smoke_limiter").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "smoke-limiter".into(),
                    name: "Smoke Limiter".into(),
                    description: "Smoke limiter map".into(),
                    rows: 10,
                    cols: 10,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "%".into(),
                    category: Some("Limiters".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("egr_map").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "egr-map".into(),
                    name: "EGR Map".into(),
                    description: "EGR duty map".into(),
                    rows: 12,
                    cols: 12,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "%".into(),
                    category: Some("EGR".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("torque_limiter").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "torque-limiter".into(),
                    name: "Torque Limiter".into(),
                    description: "Torque limit map".into(),
                    rows: 12,
                    cols: 12,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "Nm".into(),
                    category: Some("Limiters".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if let Some(addr) = addrs.get("start_of_injection").and_then(|v| v.as_str()) {
                tables.push(TableDef {
                    id: "soi".into(),
                    name: "Start of Injection".into(),
                    description: "SOI timing map".into(),
                    rows: 12,
                    cols: 12,
                    addr: addr.into(),
                    data_type: "UWORD".into(),
                    math: "x*0.1".into(),
                    units: "deg".into(),
                    category: Some("Timing".into()),
                    row_major: true,
                    msb: true,
                });
            }
            if !tables.is_empty() {
                return tables;
            }
        }
    }

    // P01 / 512KB or 128KB fallback (XDF-style community maps)
    if size == 524288 || size == 131072 {
        return vec![
            TableDef {
                id: "ve-main".into(),
                name: "Main VE".into(),
                description: "Volumetric efficiency main map - LS1 P01".into(),
                rows: 16,
                cols: 16,
                addr: "0x4000".into(),
                data_type: "UBYTE".into(),
                math: "x*0.5".into(),
                units: "%".into(),
                category: Some("Fuel".into()),
                row_major: true,
                msb: true,
            },
            TableDef {
                id: "spark-advance".into(),
                name: "Spark Advance".into(),
                description: "Base spark timing map".into(),
                rows: 12,
                cols: 14,
                addr: "0x2000".into(),
                data_type: "UBYTE".into(),
                math: "(x-40)/2".into(),
                units: "deg".into(),
                category: Some("Ignition".into()),
                row_major: true,
                msb: true,
            },
            TableDef {
                id: "idle-rpm".into(),
                name: "Idle Target RPM".into(),
                description: "Target idle speed vs temp".into(),
                rows: 1,
                cols: 8,
                addr: "0x1A00".into(),
                data_type: "UWORD".into(),
                math: "x".into(),
                units: "RPM".into(),
                category: Some("Idle".into()),
                row_major: true,
                msb: true,
            },
        ];
    }

    // 2MB diesel generic fallback (if no refined)
    if size == 2097152 {
        return vec![
            TableDef {
                id: "driver-wish".into(),
                name: "Driver Wish (Torque)".into(),
                description: "Driver requested torque".into(),
                rows: 16,
                cols: 16,
                addr: "0x80000".into(),
                data_type: "UWORD".into(),
                math: "x*0.1".into(),
                units: "Nm".into(),
                category: Some("Torque".into()),
                row_major: true,
                msb: true,
            },
            TableDef {
                id: "inj-quantity".into(),
                name: "Injection Quantity".into(),
                description: "IQ main map".into(),
                rows: 16,
                cols: 16,
                addr: "0x82000".into(),
                data_type: "UWORD".into(),
                math: "x*0.01".into(),
                units: "mm3".into(),
                category: Some("Fuel".into()),
                row_major: true,
                msb: true,
            },
        ];
    }

    Vec::new()
}
