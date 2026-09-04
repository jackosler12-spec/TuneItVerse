//! ecu_database.rs — Backend database of known ECUs for TuneItVerse
//!
//! Embeds family JSON from reference/ecu_database/. Lookup by family / OS ID
//! drives checksum dispatch, security, maps, and protocol hints.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

const P01_JSON: &str = include_str!("../../reference/ecu_database/p01_0411.json");
const EDC16_JSON: &str = include_str!("../../reference/ecu_database/edc16c41_nissan_patrol.json");
const P59_JSON: &str = include_str!("../../reference/ecu_database/gm_p59.json");
const MED17_JSON: &str = include_str!("../../reference/ecu_database/med17_common.json");
const EDC17_JSON: &str = include_str!("../../reference/ecu_database/edc17_common.json");
const ME7_JSON: &str = include_str!("../../reference/ecu_database/me7_common.json");
const DELPHI_JSON: &str = include_str!("../../reference/ecu_database/delphi_dcm.json");
const SID803_JSON: &str = include_str!("../../reference/ecu_database/siemens_sid803.json");
const HONDA_JSON: &str = include_str!("../../reference/ecu_database/honda_keihin.json");

pub fn load_ecu_database() -> Vec<EcuDbEntry> {
    let mut db = Vec::new();
    for raw in [P01_JSON, EDC16_JSON, P59_JSON, MED17_JSON, EDC17_JSON, ME7_JSON, DELPHI_JSON, SID803_JSON, HONDA_JSON] {
        if let Ok(entry) = serde_json::from_str::<EcuDbEntry>(raw) {
            db.push(entry);
        }
    }
    db
}

pub fn get_ecu_by_family(family: &str) -> Option<EcuDbEntry> {
    load_ecu_database()
        .into_iter()
        .find(|e| e.ecu_family.eq_ignore_ascii_case(family))
}

pub fn get_ecu_by_os_id(os_id: &str) -> Option<EcuDbEntry> {
    let os = os_id.to_ascii_uppercase();
    load_ecu_database().into_iter().find(|e| {
        e.part_numbers_or_os_ids.iter().any(|id| id.to_ascii_uppercase().contains(&os))
            || e.display_name.to_ascii_uppercase().contains(&os)
    })
}

pub fn list_supported_ecu_families() -> Vec<String> {
    load_ecu_database().into_iter().map(|e| e.ecu_family).collect()
}

pub fn get_ecu_by_bin_size(size: usize) -> Option<EcuDbEntry> {
    load_ecu_database()
        .into_iter()
        .find(|e| e.bin_size_bytes as usize == size)
}

fn push_table(tables: &mut Vec<TableDef>, seen: &mut HashSet<String>, def: TableDef) {
    if seen.insert(def.id.clone()) {
        tables.push(def);
    }
}

fn tables_from_addrs(entry: &EcuDbEntry, addrs: &serde_json::Value, tables: &mut Vec<TableDef>, seen: &mut HashSet<String>) {
    let fam = entry.ecu_family.as_str();
    let specs: &[(&str, &str, &str, u16, u16, &str, &str, &str, &str)] = &[
        ("driver_wish", "driver-wish", "Driver Wish (Torque)", 16, 16, "UWORD", "x*0.1", "Nm", "Torque"),
        ("injection_quantity", "inj-quantity", "Injection Quantity", 16, 16, "UWORD", "x*0.01", "mm3", "Fuel"),
        ("boost_setpoint", "boost-setpoint", "Boost Setpoint", 12, 12, "UWORD", "x*0.1", "mbar", "Boost"),
        ("boost_target", "boost-target", "Boost Target", 12, 12, "UWORD", "x*0.1", "mbar", "Boost"),
        ("rail_pressure", "rail-pressure", "Rail Pressure", 12, 12, "UWORD", "x", "bar", "Fuel"),
        ("vgt_duty", "vgt-duty", "VGT Duty", 12, 12, "UWORD", "x*0.1", "%", "Boost"),
        ("smoke_limiter", "smoke-limiter", "Smoke Limiter", 10, 10, "UWORD", "x*0.1", "%", "Limiters"),
        ("egr_map", "egr-map", "EGR Map", 12, 12, "UWORD", "x*0.1", "%", "EGR"),
        ("torque_limiter", "torque-limiter", "Torque Limiter", 12, 12, "UWORD", "x*0.1", "Nm", "Limiters"),
        ("start_of_injection", "soi", "Start of Injection", 12, 12, "UWORD", "x*0.1", "deg", "Timing"),
        ("ignition_timing", "ignition-timing", "Ignition Timing", 16, 16, "UWORD", "x*0.1", "deg", "Ignition"),
        ("fuel_ve", "fuel-ve", "Fuel / VE", 16, 16, "UWORD", "x*0.01", "%", "Fuel"),
    ];
    for (key, id, name, rows, cols, dtype, math, units, cat) in specs {
        if let Some(addr) = addrs.get(*key).and_then(|v| v.as_str()) {
            push_table(tables, seen, TableDef {
                id: (*id).into(),
                name: (*name).into(),
                description: format!("{} — {}", name, fam),
                rows: *rows,
                cols: *cols,
                addr: addr.into(),
                data_type: (*dtype).into(),
                math: (*math).into(),
                units: (*units).into(),
                category: Some((*cat).into()),
                row_major: true,
                msb: true,
            });
        }
    }
}

pub fn get_tables_for_bin_size(size: usize) -> Vec<TableDef> {
    let mut tables = Vec::new();
    let mut seen = HashSet::new();
    for entry in load_ecu_database() {
        if entry.bin_size_bytes as usize != size {
            continue;
        }
        if let Some(ref addrs) = entry.maps_and_xdf.refined_map_addrs {
            tables_from_addrs(&entry, addrs, &mut tables, &mut seen);
        }
    }
    if !tables.is_empty() {
        return tables;
    }

    if size == 524288 || size == 131072 {
        return vec![
            TableDef {
                id: "ve-main".into(),
                name: "Main VE".into(),
                description: "Volumetric efficiency main map - LS1 P01".into(),
                rows: 16, cols: 16, addr: "0x4000".into(), data_type: "UBYTE".into(),
                math: "x*0.5".into(), units: "%".into(), category: Some("Fuel".into()),
                row_major: true, msb: true,
            },
            TableDef {
                id: "spark-advance".into(),
                name: "Spark Advance".into(),
                description: "Base spark timing map".into(),
                rows: 12, cols: 14, addr: "0x2000".into(), data_type: "UBYTE".into(),
                math: "(x-40)/2".into(), units: "deg".into(), category: Some("Ignition".into()),
                row_major: true, msb: true,
            },
            TableDef {
                id: "idle-rpm".into(),
                name: "Idle Target RPM".into(),
                description: "Target idle speed vs temp".into(),
                rows: 1, cols: 8, addr: "0x1A00".into(), data_type: "UWORD".into(),
                math: "x".into(), units: "RPM".into(), category: Some("Idle".into()),
                row_major: true, msb: true,
            },
        ];
    }

    if size == 1048576 {
        return vec![
            TableDef {
                id: "ignition-timing".into(),
                name: "Ignition Timing".into(),
                description: "ME7 community start hint — confirm on your dump".into(),
                rows: 16, cols: 16, addr: "0x1C000".into(), data_type: "UWORD".into(),
                math: "x*0.1".into(), units: "deg".into(), category: Some("Ignition".into()),
                row_major: true, msb: true,
            },
            TableDef {
                id: "fuel-ve".into(),
                name: "Fuel / VE".into(),
                description: "ME7 community start hint — confirm on your dump".into(),
                rows: 16, cols: 16, addr: "0x1A000".into(), data_type: "UWORD".into(),
                math: "x*0.01".into(), units: "%".into(), category: Some("Fuel".into()),
                row_major: true, msb: true,
            },
        ];
    }

    if size == 2097152 {
        return vec![
            TableDef {
                id: "driver-wish".into(),
                name: "Driver Wish (Torque)".into(),
                description: "Driver requested torque".into(),
                rows: 16, cols: 16, addr: "0x80000".into(), data_type: "UWORD".into(),
                math: "x*0.1".into(), units: "Nm".into(), category: Some("Torque".into()),
                row_major: true, msb: true,
            },
            TableDef {
                id: "inj-quantity".into(),
                name: "Injection Quantity".into(),
                description: "IQ main map".into(),
                rows: 16, cols: 16, addr: "0x82000".into(), data_type: "UWORD".into(),
                math: "x*0.01".into(), units: "mm3".into(), category: Some("Fuel".into()),
                row_major: true, msb: true,
            },
        ];
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_includes_me7_and_delphi() {
        let fams = list_supported_ecu_families();
        assert!(fams.iter().any(|f| f == "ME7_COMMON"));
        assert!(fams.iter().any(|f| f == "DELPHI_DCM"));
        assert!(fams.iter().any(|f| f == "SIEMENS_SID803"));
        assert!(fams.iter().any(|f| f == "HONDA_KEIHIN"));
        assert!(get_ecu_by_bin_size(1048576).is_some());
    }
}
