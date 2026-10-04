//! Community definition packs. Hints only — a pack cannot enable a write path.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMap {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "one")]
    pub rows: u32,
    #[serde(default = "one")]
    pub cols: u32,
    #[serde(default)]
    pub scaling: String,
    #[serde(default)]
    pub notes: String,
}

fn one() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionPack {
    pub pack_id: String,
    pub ecu_family: String,
    #[serde(default)]
    pub display_name: String,
    /// Ignored on import. Write stays fail-closed in ecu_database.
    #[serde(default)]
    pub write_allowed: bool,
    #[serde(default)]
    pub maps: Vec<PackMap>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedPack {
    pub pack_id: String,
    pub ecu_family: String,
    pub display_name: String,
    pub map_count: usize,
    pub write_allowed: bool,
    pub notes: String,
    pub maps: Vec<PackMap>,
}

static PACKS: Mutex<Vec<ImportedPack>> = Mutex::new(Vec::new());

pub fn validate_pack(pack: &DefinitionPack) -> Result<(), String> {
    if pack.pack_id.trim().is_empty() {
        return Err("pack_id is required".into());
    }
    if pack.ecu_family.trim().is_empty() {
        return Err("ecu_family is required".into());
    }
    if pack.maps.len() > 512 {
        return Err("definition pack exceeds 512 maps".into());
    }
    for m in &pack.maps {
        if m.id.trim().is_empty() || m.name.trim().is_empty() {
            return Err("each map needs id and name".into());
        }
        if m.rows == 0 || m.cols == 0 || m.rows > 256 || m.cols > 256 {
            return Err(format!("map {} has invalid dimensions", m.id));
        }
    }
    Ok(())
}

pub fn import_pack_value(pack: DefinitionPack) -> Result<ImportedPack, String> {
    validate_pack(&pack)?;
    let imported = ImportedPack {
        pack_id: pack.pack_id,
        ecu_family: pack.ecu_family,
        display_name: pack.display_name,
        map_count: pack.maps.len(),
        write_allowed: false,
        notes: if pack.write_allowed {
            format!("{} (pack asked for write; ignored)", pack.notes)
        } else {
            pack.notes
        },
        maps: pack.maps,
    };
    let mut g = PACKS.lock().map_err(|e| e.to_string())?;
    g.retain(|p| p.pack_id != imported.pack_id);
    g.push(imported.clone());
    Ok(imported)
}

#[tauri::command]
pub fn import_definition_pack(json_text: String) -> Result<String, String> {
    let pack: DefinitionPack = serde_json::from_str(&json_text).map_err(|e| format!("definition pack JSON: {e}"))?;
    let imported = import_pack_value(pack)?;
    serde_json::to_string(&imported).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_definition_packs() -> Result<String, String> {
    let g = PACKS.lock().map_err(|e| e.to_string())?;
    serde_json::to_string(&*g).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn operational_self_check() -> Result<String, String> {
    let families = crate::ecu_database::list_supported_ecu_families();
    let write: Vec<&str> = families.iter().map(|s| s.as_str()).filter(|f| crate::ecu_database::write_path_live(f)).collect();
    let help = crate::scripting::list_script_helpers().unwrap_or_default();
    let sample = crate::voltage_watch::parse_at_rv("12.8V");
    let pack_ok = validate_pack(&DefinitionPack {
        pack_id: "self-check".into(),
        ecu_family: "P01_0411".into(),
        display_name: String::new(),
        write_allowed: true,
        maps: vec![PackMap { id: "spark".into(), name: "Spark".into(), offset: 0, rows: 1, cols: 1, scaling: String::new(), notes: String::new() }],
        notes: String::new(),
    }).is_ok();
    let report = serde_json::json!({
        "version": crate::APP_VERSION,
        "families": families.len(),
        "write_enabled": write,
        "write_enabled_count": write.len(),
        "script_helpers_ok": !help.is_empty(),
        "at_rv_parser_ok": sample == Some(12.8),
        "definition_pack_ok": pack_ok,
        "p59_write_blocked": !crate::ecu_database::write_path_live("GM_P59"),
        "honda_write_blocked": !families.iter().any(|f| f.to_ascii_uppercase().contains("HONDA") && crate::ecu_database::write_path_live(f)),
        "notes": "Self-check is offline. It does not talk to an ECU and does not enable write.",
        "inspect_ok": crate::bin_inspect::profile_bin(&[0u8; 32]).get("entropy").and_then(|v| v.as_f64()) == Some(0.0),
        "log_studio_ok": crate::log_studio::self_check_ok()
    });
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_cannot_enable_write() {
        let imported = import_pack_value(DefinitionPack {
            pack_id: "test-pack".into(),
            ecu_family: "EDC16C41".into(),
            display_name: "Patrol hints".into(),
            write_allowed: true,
            maps: vec![PackMap { id: "iq".into(), name: "IQ".into(), offset: 0x1000, rows: 16, cols: 16, scaling: "x*0.1".into(), notes: String::new() }],
            notes: "personal".into(),
        }).unwrap();
        assert!(!imported.write_allowed);
        assert_eq!(imported.map_count, 1);
    }

    #[test]
    fn rejects_empty_family() {
        let err = validate_pack(&DefinitionPack {
            pack_id: "x".into(),
            ecu_family: "  ".into(),
            display_name: String::new(),
            write_allowed: false,
            maps: vec![],
            notes: String::new(),
        });
        assert!(err.is_err());
    }
}
