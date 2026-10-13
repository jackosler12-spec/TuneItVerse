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
    /// Calibration segment that guided flash may write (from authoritative repo data only).
    #[serde(default)]
    pub flash_write_segment: Option<FlashSegmentRaw>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct FlashSegmentRaw {
    pub name: String,
    pub start: String,
    pub length: String,
    pub image_size: usize,
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Parsed calibration segment: flash address == file offset in a full image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashSegment {
    pub start: u32,
    pub length: u32,
    pub image_size: usize,
}

impl FlashSegment {
    /// Validate the image size and return the bytes to write.
    pub fn slice<'a>(&self, image: &'a [u8]) -> Result<&'a [u8], String> {
        if image.len() != self.image_size {
            return Err(format!(
                "BIN size {} does not match family image size {}. Refusing write.",
                image.len(), self.image_size
            ));
        }
        let s = self.start as usize;
        let e = s.checked_add(self.length as usize).filter(|&e| e <= image.len())
            .ok_or_else(|| "Segment exceeds image. Refusing write.".to_string())?;
        Ok(&image[s..e])
    }
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
const EDC15_JSON: &str = include_str!("../../reference/ecu_database/edc15_common.json");
const ME9_JSON: &str = include_str!("../../reference/ecu_database/me9_common.json");
const SIMOS18_JSON: &str = include_str!("../../reference/ecu_database/simos18.json");
const TRIONIC8_JSON: &str = include_str!("../../reference/ecu_database/trionic8.json");
const TRANSTRON_JSON: &str = include_str!("../../reference/ecu_database/transtron_4hk1.json");

pub fn load_ecu_database() -> Vec<EcuDbEntry> {
    let mut db = Vec::new();
    for raw in [P01_JSON, EDC16_JSON, P59_JSON, MED17_JSON, EDC17_JSON, ME7_JSON, DELPHI_JSON, SID803_JSON, HONDA_JSON, EDC15_JSON, ME9_JSON, SIMOS18_JSON, TRIONIC8_JSON, TRANSTRON_JSON] {
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

/// Writable calibration segment for `family` from the ECU database.
/// Errors when the family is unknown or has no `flash_write_segment`.
pub fn flash_segment(family: &str) -> Result<FlashSegment, String> {
    let e = get_ecu_by_family(family)
        .ok_or_else(|| format!("ECU family {} not in database; no flash segment. Refusing.", family))?;
    let raw = e.flash_write_segment
        .ok_or_else(|| format!("ECU database has no flash_write_segment for {}. Refusing.", e.ecu_family))?;
    let start = parse_hex_u32(&raw.start).ok_or_else(|| format!("Bad segment start '{}'", raw.start))?;
    let length = parse_hex_u32(&raw.length).ok_or_else(|| format!("Bad segment length '{}'", raw.length))?;
    if length == 0 || start as usize + length as usize > raw.image_size {
        return Err(format!("Segment for {} is outside its image size. Refusing.", e.ecu_family));
    }
    Ok(FlashSegment { start, length, image_size: raw.image_size })
}

/// Calibration start address (segment start) for flash/live verify.
pub fn flash_cal_address(family: &str) -> Result<u32, String> {
    flash_segment(family).map(|s| s.start)
}

fn parse_hex_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    let h = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X"))?;
    u32::from_str_radix(&h.replace('_', ""), 16).ok()
}

/// Live write is advertised only for families with a measured corrector + kernel path.
pub fn write_path_live(family: &str) -> bool {
    family.eq_ignore_ascii_case("P01_0411")
}

pub fn write_families() -> &'static [&'static str] {
    &["P01_0411"]
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
    let specs: &[(&str, &str, &str, usize, usize, &str, &str, &str, &str)] = &[
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
                family: Some(fam.to_string()),
                ..Default::default()
            });
        }
    }
}

#[derive(Serialize)]
pub struct AutoTablesResult {
    pub tables: Vec<TableDef>,
    pub located: usize,
    pub missing: usize,
    pub pack: String,
    pub note: String,
}

pub fn tables_for_bin(data: &[u8]) -> AutoTablesResult {
    if crate::cs_guard::honda_blocks_p01_corrector(data) {
        return AutoTablesResult {
            tables: vec![],
            located: 0,
            missing: 0,
            pack: String::new(),
            note: "Honda OS string. P01 TableSeek pack is not applied.".into(),
        };
    }
    let p01_sized = crate::checksum_sizes::is_p01_size(data.len());
    if p01_sized || crate::cs_guard::looks_like_gm_p01(data) {
        let (tables, located, missing) = crate::tableseek::locate_p01_tables(data);
        return AutoTablesResult {
            note: format!("P01/P59 TableSeek: {} located, {} not in this dump.", located, missing),
            pack: "tableseek-p01-p59".into(),
            tables,
            located,
            missing,
        };
    }
    let size_tables = get_tables_for_bin_size(data.len());
    let n = size_tables.len();
    AutoTablesResult {
        note: if n == 0 {
            "No catalog pack for this size. Load an XDF/A2L, or confirm the dump has a GM P01 OS string.".into()
        } else {
            format!("{} catalog tables by BIN size (not P01 TableSeek).", n)
        },
        pack: "size-catalog".into(),
        located: n,
        missing: 0,
        tables: size_tables,
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

    if size == 1048576 {
        return vec![
            TableDef {
                id: "ignition-timing".into(),
                name: "Ignition Timing".into(),
                description: "ME7 community start hint — confirm on your dump".into(),
                rows: 16, cols: 16, addr: "0x1C000".into(), data_type: "UWORD".into(),
                math: "x*0.1".into(), units: "deg".into(), category: Some("Ignition".into()),
                row_major: true, msb: true, ..Default::default()
            },
            TableDef {
                id: "fuel-ve".into(),
                name: "Fuel / VE".into(),
                description: "ME7 community start hint — confirm on your dump".into(),
                rows: 16, cols: 16, addr: "0x1A000".into(), data_type: "UWORD".into(),
                math: "x*0.01".into(), units: "%".into(), category: Some("Fuel".into()),
                row_major: true, msb: true, ..Default::default()
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
                row_major: true, msb: true, ..Default::default()
            },
            TableDef {
                id: "inj-quantity".into(),
                name: "Injection Quantity".into(),
                description: "IQ main map".into(),
                rows: 16, cols: 16, addr: "0x82000".into(), data_type: "UWORD".into(),
                math: "x*0.01".into(), units: "mm3".into(), category: Some("Fuel".into()),
                row_major: true, msb: true, ..Default::default()
            },
        ];
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p01_segment_is_pcmhammer_calibration() {
        // reference/PcmHammer_debugLog_20260504@180810.txt: 008000-01FFFF Calibration
        let seg = flash_segment("P01_0411").unwrap();
        assert_eq!(seg, FlashSegment { start: 0x8000, length: 0x18000, image_size: 0x80000 });
        assert_eq!(flash_cal_address("p01_0411").unwrap(), 0x8000);
        // Old hardcoded 0x20000 was the start of the OperatingSystem segment.
        assert_ne!(seg.start, 0x20000);
    }

    #[test]
    fn segment_refused_without_authoritative_entry() {
        // EDC16C41: no segment boundaries in repo data → refused.
        assert!(flash_segment("EDC16C41").unwrap_err().contains("no flash_write_segment"));
        assert!(flash_segment("ME7_COMMON").is_err());
        assert!(flash_segment("EDC17_COMMON").is_err());
        assert!(flash_segment("NOT_A_FAMILY").unwrap_err().contains("not in database"));
    }

    #[test]
    fn segment_slice_checks_size() {
        let seg = flash_segment("P01_0411").unwrap();
        let mut img = vec![0u8; 0x80000];
        img[0x8000] = 0xAA; img[0x1FFFF] = 0xBB;
        let w = seg.slice(&img).unwrap();
        assert_eq!(w.len(), 0x18000);
        assert_eq!((w[0], w[w.len() - 1]), (0xAA, 0xBB));
        assert!(seg.slice(&vec![0u8; 0x20000]).unwrap_err().contains("does not match"));
    }

    #[test]
    fn hex_parse() {
        assert_eq!(parse_hex_u32("0x0002_0000"), Some(0x20000));
        assert_eq!(parse_hex_u32("20000"), None);
    }
    #[test]
    fn catalog_includes_me7_and_delphi() {
        let fams = list_supported_ecu_families();
        assert!(fams.iter().any(|f| f == "ME7_COMMON"));
        assert!(fams.iter().any(|f| f == "DELPHI_DCM"));
        assert!(fams.iter().any(|f| f == "SIEMENS_SID803"));
        assert!(fams.iter().any(|f| f == "HONDA_KEIHIN"));
        assert!(fams.iter().any(|f| f == "EDC15_COMMON"));
        assert!(fams.iter().any(|f| f == "ME9_COMMON"));
        assert!(fams.iter().any(|f| f == "SIMOS18"));
        assert!(fams.iter().any(|f| f == "TRIONIC8"));
        assert!(fams.iter().any(|f| f == "TRANSTRON_4HK1"));
        assert!(get_ecu_by_bin_size(1048576).is_some());
        assert!(get_ecu_by_family("SIMOS18").is_some());
        assert_eq!(get_ecu_by_family("SIMOS18").unwrap().bin_size_bytes, 4_194_304);
        assert!(get_ecu_by_family("TRANSTRON_4HK1").is_some());
        assert!(write_path_live("P01_0411"));
        assert!(!write_path_live("edc16c41"));
        assert!(!write_path_live("EDC17_COMMON"));
        assert!(!write_path_live("GM_P59"));
        assert!(!write_path_live("HONDA_KEIHIN"));
    }
    #[test]
    fn honda_512k_does_not_get_p01_pack() {
        let mut img = vec![0u8; 524288];
        img[0x40..0x48].copy_from_slice(b"37820-PR");
        let r = tables_for_bin(&img);
        assert!(r.tables.is_empty());
        assert!(r.note.contains("Honda"));
    }
    #[test]
    fn blank_512k_is_not_fake_ve_spark() {
        let img = vec![0u8; 524288];
        let r = tables_for_bin(&img);
        assert!(!r.tables.iter().any(|t| t.id == "ve-main"));
        assert_eq!(r.pack, "tableseek-p01-p59");
        assert_eq!(r.located, 0);
    }
}
