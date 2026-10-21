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
    /// Calibration IDs seen live on a vehicle that *may* belong to this family.
    /// Observation only: never used to set the family (see `candidate_for_cal_id`).
    #[serde(default)]
    pub observed_cal_ids: Vec<String>,
    /// Citations for the entry (required, non-empty; see `validate_entry`).
    #[serde(default)]
    pub sources: Vec<String>,
    /// True only with a recorded bench test. Nothing in this tree is bench-tested.
    #[serde(default)]
    pub hardware_tested: bool,
    /// Per-capability status + source (required; see `validate_entry`).
    #[serde(default)]
    pub capabilities: Option<Capabilities>,
}

/// Allowed values of `CapabilityStatus::status`.
///
/// - `verified_dump`: checked against a real image in reference/ (or the owner's dump)
/// - `verified_log`: checked against a real logged session in reference/
/// - `public_source`: taken from cited public source code/docs; not checked here
/// - `reference_doc`: from a document in reference/; not checked on hardware
/// - `experimental`: implemented behind an opt-in flag; not bench-tested
/// - `report_only`: the app reports but refuses to change anything
/// - `unverified`: listed, no source/dump/log backs it
/// - `not_supported`: the app refuses
pub const STATUS_ENUM: &[&str] = &[
    "verified_dump", "verified_log", "public_source", "reference_doc",
    "experimental", "report_only", "unverified", "not_supported",
];

/// Capabilities every entry must describe.
pub const CAPABILITY_KEYS: &[&str] = &["identify", "read", "checksum", "seed_key", "write", "map_hints"];

/// Families whose checksum corrector is verified against a real image.
pub const CHECKSUM_CORRECTOR_FAMILIES: &[&str] = &["P01_0411"];

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct CapabilityStatus {
    pub status: String,
    pub source: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Capabilities {
    pub identify: Option<CapabilityStatus>,
    pub read: Option<CapabilityStatus>,
    pub checksum: Option<CapabilityStatus>,
    pub seed_key: Option<CapabilityStatus>,
    pub write: Option<CapabilityStatus>,
    pub map_hints: Option<CapabilityStatus>,
}

impl Capabilities {
    pub fn get(&self, key: &str) -> Option<&CapabilityStatus> {
        match key {
            "identify" => self.identify.as_ref(),
            "read" => self.read.as_ref(),
            "checksum" => self.checksum.as_ref(),
            "seed_key" => self.seed_key.as_ref(),
            "write" => self.write.as_ref(),
            "map_hints" => self.map_hints.as_ref(),
            _ => None,
        }
    }
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
const P10_JSON: &str = include_str!("../../reference/ecu_database/gm_p10.json");
const P12_JSON: &str = include_str!("../../reference/ecu_database/gm_p12.json");
const INDEX_JSON: &str = include_str!("../../reference/ecu_database/index.json");

/// (file name, embedded JSON) in load order. Order matters for size lookups.
const DB_FILES: &[(&str, &str)] = &[
    ("p01_0411.json", P01_JSON), ("edc16c41_nissan_patrol.json", EDC16_JSON), ("gm_p59.json", P59_JSON),
    ("med17_common.json", MED17_JSON), ("edc17_common.json", EDC17_JSON), ("me7_common.json", ME7_JSON),
    ("delphi_dcm.json", DELPHI_JSON), ("siemens_sid803.json", SID803_JSON), ("honda_keihin.json", HONDA_JSON),
    ("edc15_common.json", EDC15_JSON), ("me9_common.json", ME9_JSON), ("simos18.json", SIMOS18_JSON),
    ("trionic8.json", TRIONIC8_JSON), ("transtron_4hk1.json", TRANSTRON_JSON),
    ("gm_p10.json", P10_JSON), ("gm_p12.json", P12_JSON),
];

struct LoadedDb {
    entries: Vec<EcuDbEntry>,
    errors: Vec<String>,
}

fn non_empty(v: &str) -> bool {
    !v.trim().is_empty()
}

/// Schema check for one entry. Fail-closed: an entry with any error is not loaded.
pub fn validate_entry(e: &EcuDbEntry) -> Vec<String> {
    let mut errs = Vec::new();
    let fam = e.ecu_family.as_str();
    let tag = |m: String| format!("{}: {}", if fam.is_empty() { "<no ecu_family>" } else { fam }, m);
    if fam.is_empty() || !fam.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        errs.push(tag("ecu_family must be non-empty [A-Z0-9_]".into()));
    }
    for (name, v) in [("display_name", &e.display_name), ("hardware", &e.hardware), ("protocol", &e.protocol), ("notes", &e.notes)] {
        if !non_empty(v) {
            errs.push(tag(format!("{} is required", name)));
        }
    }
    if e.bin_size_bytes == 0 {
        errs.push(tag("bin_size_bytes must be > 0".into()));
    }
    if e.sources.is_empty() || e.sources.iter().any(|s| !non_empty(s)) {
        errs.push(tag("sources must be a non-empty list of citations".into()));
    }
    let Some(caps) = e.capabilities.as_ref() else {
        errs.push(tag("capabilities block is required".into()));
        return errs;
    };
    for key in CAPABILITY_KEYS {
        match caps.get(key) {
            None => errs.push(tag(format!("capabilities.{} is required", key))),
            Some(c) => {
                if !STATUS_ENUM.contains(&c.status.as_str()) {
                    errs.push(tag(format!("capabilities.{}.status '{}' not in {:?}", key, c.status, STATUS_ENUM)));
                }
                if c.source.trim().len() < 8 {
                    errs.push(tag(format!("capabilities.{}.source must cite where the status comes from", key)));
                }
            }
        }
    }
    let status = |k: &str| caps.get(k).map(|c| c.status.as_str()).unwrap_or("");
    // Write: only code-enabled families, and only ever experimental (nothing is bench-tested).
    let in_write = write_families().iter().any(|f| f.eq_ignore_ascii_case(fam));
    match (in_write, status("write")) {
        (true, "experimental") | (false, "not_supported") => {}
        (true, s) => errs.push(tag(format!("write family must be 'experimental' (not bench-tested), got '{}'", s))),
        (false, s) => errs.push(tag(format!("write must be 'not_supported' outside write_families(), got '{}'", s))),
    }
    let cs = status("checksum");
    let has_corrector = CHECKSUM_CORRECTOR_FAMILIES.iter().any(|f| f.eq_ignore_ascii_case(fam));
    if !has_corrector && matches!(cs, "verified_dump" | "verified_log" | "public_source" | "experimental") {
        errs.push(tag(format!("checksum '{}' claims a corrector this family does not have", cs)));
    }
    if status("map_hints") == "not_supported" && e.maps_and_xdf.refined_map_addrs.is_some() {
        errs.push(tag("map_hints is not_supported but refined_map_addrs is present".into()));
    }
    if e.hardware_tested {
        errs.push(tag("hardware_tested=true needs a recorded bench test; none exists in this tree".into()));
    }
    errs
}

/// Cross-check index.json against the embedded family files.
fn validate_index(entries: &[(&str, EcuDbEntry)]) -> Vec<String> {
    let mut errs = Vec::new();
    let idx: serde_json::Value = match serde_json::from_str(INDEX_JSON) {
        Ok(v) => v,
        Err(e) => return vec![format!("index.json: {}", e)],
    };
    let fams = idx.get("families").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    if let Some(en) = idx.get("status_enum").and_then(|v| v.as_array()) {
        let listed: Vec<&str> = en.iter().filter_map(|v| v.as_str()).collect();
        if listed != STATUS_ENUM {
            errs.push("index.json: status_enum differs from ecu_database::STATUS_ENUM".into());
        }
    } else {
        errs.push("index.json: status_enum missing".into());
    }
    for (file, e) in entries {
        match fams.iter().find(|f| f.get("file").and_then(|v| v.as_str()) == Some(*file)) {
            None => errs.push(format!("index.json: no entry for {}", file)),
            Some(f) => {
                if f.get("id").and_then(|v| v.as_str()) != Some(e.ecu_family.as_str()) {
                    errs.push(format!("index.json: id for {} is not {}", file, e.ecu_family));
                }
                if f.get("bin_size_bytes").and_then(|v| v.as_u64()) != Some(e.bin_size_bytes as u64) {
                    errs.push(format!("index.json: bin_size_bytes for {} differs from the family file", e.ecu_family));
                }
            }
        }
    }
    if fams.len() != entries.len() {
        errs.push(format!("index.json lists {} families, {} are embedded", fams.len(), entries.len()));
    }
    errs
}

fn loaded() -> &'static LoadedDb {
    static DB: std::sync::OnceLock<LoadedDb> = std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut errors = Vec::new();
        let mut parsed: Vec<(&str, EcuDbEntry)> = Vec::new();
        for (file, raw) in DB_FILES {
            match serde_json::from_str::<EcuDbEntry>(raw) {
                Ok(e) => parsed.push((file, e)),
                Err(err) => errors.push(format!("{}: parse error: {}", file, err)),
            }
        }
        errors.extend(validate_index(&parsed));
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for (file, e) in parsed {
            let errs = validate_entry(&e);
            if !seen.insert(e.ecu_family.to_ascii_uppercase()) {
                errors.push(format!("{}: duplicate ecu_family {}", file, e.ecu_family));
                continue;
            }
            if errs.is_empty() {
                entries.push(e);
            } else {
                errors.extend(errs.into_iter().map(|m| format!("{} ({}) — entry not loaded", m, file)));
            }
        }
        LoadedDb { entries, errors }
    })
}

/// Valid entries only. Invalid entries are dropped (fail-closed) and listed by `validation_errors`.
pub fn load_ecu_database() -> Vec<EcuDbEntry> {
    loaded().entries.clone()
}

/// Schema/index errors found when the database was loaded. Empty in a good build.
pub fn validation_errors() -> Vec<String> {
    loaded().errors.clone()
}

/// Per-family capability matrix for the UI (read/identify/checksum/write + basis).
pub fn capability_matrix() -> serde_json::Value {
    let fams: Vec<serde_json::Value> = loaded().entries.iter().map(|e| {
        serde_json::json!({
            "family": e.ecu_family,
            "display_name": e.display_name,
            "bin_size_bytes": e.bin_size_bytes,
            "protocol": e.protocol,
            "hardware_tested": e.hardware_tested,
            "write_path_in_code": write_path_live(&e.ecu_family),
            "write_requires": if write_path_live(&e.ecu_family) {
                serde_json::json!(["FullImage backup", "voltage gate", "experimental_p01_write flag", "typed phrase WRITE CALIBRATION TO PCM"])
            } else { serde_json::json!([]) },
            "checksum_corrector": CHECKSUM_CORRECTOR_FAMILIES.iter().any(|f| f.eq_ignore_ascii_case(&e.ecu_family)),
            "capabilities": e.capabilities,
            "sources": e.sources,
        })
    }).collect();
    serde_json::json!({
        "status_enum": STATUS_ENUM,
        "capability_keys": CAPABILITY_KEYS,
        "families": fams,
        "validation_errors": validation_errors(),
        "bench_tested_anything": false,
    })
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

/// Family a live Mode 09 cal ID was *observed* alongside, with the caveat. This
/// is a hint for the UI only; it never confirms the ECU family.
pub fn candidate_for_cal_id(cal_id: &str) -> Option<(String, String)> {
    let c = cal_id.trim().to_ascii_uppercase();
    if c.is_empty() {
        return None;
    }
    load_ecu_database().into_iter().find_map(|e| {
        e.observed_cal_ids
            .iter()
            .any(|o| o.eq_ignore_ascii_case(&c))
            .then(|| (e.ecu_family.clone(), format!("cal ID {} was observed on a vehicle listed for {}; hardware not confirmed", c, e.ecu_family)))
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
                description: format!("UNVERIFIED hint (no cited source) — {} — {}. Confirm the address on your own dump/XDF before editing.", name, fam),
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
            format!("{} UNVERIFIED catalog hints by BIN size (no cited source; not P01 TableSeek). Confirm addresses before editing.", n)
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
                description: "UNVERIFIED hint (no cited source) — confirm on your own dump/XDF before editing".into(),
                rows: 16, cols: 16, addr: "0x1C000".into(), data_type: "UWORD".into(),
                math: "x*0.1".into(), units: "deg".into(), category: Some("Ignition".into()),
                row_major: true, msb: true, ..Default::default()
            },
            TableDef {
                id: "fuel-ve".into(),
                name: "Fuel / VE".into(),
                description: "UNVERIFIED hint (no cited source) — confirm on your own dump/XDF before editing".into(),
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
                description: "UNVERIFIED hint (no cited source) — confirm on your own dump/XDF before editing".into(),
                rows: 16, cols: 16, addr: "0x80000".into(), data_type: "UWORD".into(),
                math: "x*0.1".into(), units: "Nm".into(), category: Some("Torque".into()),
                row_major: true, msb: true, ..Default::default()
            },
            TableDef {
                id: "inj-quantity".into(),
                name: "Injection Quantity".into(),
                description: "UNVERIFIED hint (no cited source) — confirm on your own dump/XDF before editing".into(),
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

    #[test]
    fn patrol_cal_id_is_observed_not_confirmed() {
        // The live cal ID must not resolve to a confirmed family...
        assert!(get_ecu_by_os_id("0VS43B").is_none());
        // ...but is surfaced as an unconfirmed candidate.
        let (fam, note) = candidate_for_cal_id("0vs43b").unwrap();
        assert_eq!(fam, "EDC16C41");
        assert!(note.contains("not confirmed"));
        let raw: serde_json::Value = serde_json::from_str(EDC16_JSON).unwrap();
        assert_eq!(raw["verified"]["vehicle_mapping"], "unconfirmed");
        assert!(raw["live_observations"]["mapping"].as_str().unwrap().contains("UNCONFIRMED"));
    }

    #[test]
    fn every_db_file_loads_and_validates() {
        let errs = validation_errors();
        assert!(errs.is_empty(), "ECU DB validation errors:\n{}", errs.join("\n"));
        assert_eq!(load_ecu_database().len(), DB_FILES.len());
        assert_eq!(DB_FILES.len(), 16);
    }

    #[test]
    fn every_capability_has_enum_status_and_source() {
        for e in load_ecu_database() {
            let caps = e.capabilities.as_ref().unwrap();
            for k in CAPABILITY_KEYS {
                let c = caps.get(k).unwrap_or_else(|| panic!("{} missing {}", e.ecu_family, k));
                assert!(STATUS_ENUM.contains(&c.status.as_str()), "{} {} {}", e.ecu_family, k, c.status);
                assert!(c.source.len() >= 8);
            }
            assert!(!e.hardware_tested, "{} claims hardware_tested", e.ecu_family);
        }
    }

    fn good_entry() -> EcuDbEntry {
        serde_json::from_str::<EcuDbEntry>(ME7_JSON).unwrap()
    }

    #[test]
    fn validator_rejects_bad_entries() {
        assert!(validate_entry(&good_entry()).is_empty());

        let mut e = good_entry();
        e.capabilities = None;
        assert!(validate_entry(&e).iter().any(|m| m.contains("capabilities block")));

        let mut e = good_entry();
        e.capabilities.as_mut().unwrap().read.as_mut().unwrap().status = "probably".into();
        assert!(validate_entry(&e).iter().any(|m| m.contains("not in")));

        let mut e = good_entry();
        e.capabilities.as_mut().unwrap().identify = None;
        assert!(validate_entry(&e).iter().any(|m| m.contains("identify is required")));

        let mut e = good_entry();
        e.capabilities.as_mut().unwrap().write.as_mut().unwrap().status = "experimental".into();
        assert!(validate_entry(&e).iter().any(|m| m.contains("write must be 'not_supported'")));

        let mut e = good_entry();
        e.capabilities.as_mut().unwrap().checksum.as_mut().unwrap().status = "verified_dump".into();
        assert!(validate_entry(&e).iter().any(|m| m.contains("corrector")));

        let mut e = good_entry();
        e.capabilities.as_mut().unwrap().map_hints.as_mut().unwrap().status = "not_supported".into();
        assert!(validate_entry(&e).iter().any(|m| m.contains("refined_map_addrs")));

        let mut e = good_entry();
        e.hardware_tested = true;
        assert!(validate_entry(&e).iter().any(|m| m.contains("bench test")));

        let mut e = good_entry();
        e.sources.clear();
        assert!(validate_entry(&e).iter().any(|m| m.contains("sources")));

        let mut e = good_entry();
        e.capabilities.as_mut().unwrap().seed_key.as_mut().unwrap().source = "".into();
        assert!(validate_entry(&e).iter().any(|m| m.contains("cite")));

        let mut e = good_entry();
        e.ecu_family = "me7 common".into();
        assert!(!validate_entry(&e).is_empty());
    }

    #[test]
    fn p01_write_must_stay_experimental() {
        let mut e: EcuDbEntry = serde_json::from_str(P01_JSON).unwrap();
        assert!(validate_entry(&e).is_empty());
        assert_eq!(e.capabilities.as_ref().unwrap().write.as_ref().unwrap().status, "experimental");
        e.capabilities.as_mut().unwrap().write.as_mut().unwrap().status = "verified_dump".into();
        assert!(validate_entry(&e).iter().any(|m| m.contains("not bench-tested")));
    }

    #[test]
    fn edc16_checksum_and_write_stay_closed() {
        let e = get_ecu_by_family("EDC16C41").unwrap();
        let c = e.capabilities.unwrap();
        assert_eq!(c.checksum.unwrap().status, "report_only");
        assert_eq!(c.write.unwrap().status, "not_supported");
    }

    #[test]
    fn pcmhammer_sourced_gm_families() {
        let p59 = get_ecu_by_family("GM_P59").unwrap();
        assert_eq!(p59.bin_size_bytes, 1024 * 1024, "PcmInfo.cs: P59 ImageSize = 1024 * 1024");
        assert!(p59.maps_and_xdf.refined_map_addrs.is_none(), "unsourced P59 addresses were removed");
        let p10 = get_ecu_by_family("GM_P10").unwrap();
        assert_eq!(p10.bin_size_bytes, 512 * 1024);
        assert!(p10.sources[0].contains("033cb039"));
        let p12 = get_ecu_by_family("GM_P12").unwrap();
        assert_eq!(p12.bin_size_bytes, 1024 * 1024);
        // Every P10 OS ID listed must map to P10 in the PcmHammer table.
        for id in &p10.part_numbers_or_os_ids {
            let n: u32 = id.parse().unwrap();
            assert_eq!(crate::gm_osid::pcm_type(n), Some("P10"), "{}", id);
        }
        for f in ["GM_P59", "GM_P10", "GM_P12"] {
            assert!(!write_path_live(f));
        }
    }

    #[test]
    fn capability_matrix_shape() {
        let m = capability_matrix();
        let fams = m["families"].as_array().unwrap();
        assert_eq!(fams.len(), 16);
        let live: Vec<&str> = fams.iter().filter(|f| f["write_path_in_code"] == true).map(|f| f["family"].as_str().unwrap()).collect();
        assert_eq!(live, vec!["P01_0411"]);
        assert_eq!(m["bench_tested_anything"], false);
        assert!(m["validation_errors"].as_array().unwrap().is_empty());
        let p01 = fams.iter().find(|f| f["family"] == "P01_0411").unwrap();
        assert_eq!(p01["capabilities"]["checksum"]["status"], "verified_dump");
        assert!(p01["write_requires"].as_array().unwrap().len() >= 3);
    }

    #[test]
    fn size_catalog_tables_are_labelled_unverified() {
        let r = tables_for_bin(&vec![0u8; 2 * 1024 * 1024]);
        assert!(!r.tables.is_empty());
        assert!(r.tables.iter().all(|t| t.description.starts_with("UNVERIFIED")));
        assert!(r.note.contains("UNVERIFIED"));
    }

    #[test]
    fn index_rejects_mismatch() {
        let e: EcuDbEntry = serde_json::from_str(ME7_JSON).unwrap();
        let errs = validate_index(&[("not_listed.json", e)]);
        assert!(errs.iter().any(|m| m.contains("no entry for not_listed.json")));
    }
}
