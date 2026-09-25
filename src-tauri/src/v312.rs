//! v3.30.0 command extras: app_info, voltage, checksum report, session, packs, capabilities.
use serde_json::json;

const VERSION: &str = "3.30.0";

#[tauri::command]
pub fn app_info() -> Result<String, String> {
    Ok(json!({
        "name": "TuneItVerse",
        "version": VERSION,
        "families": crate::ecu_database::list_supported_ecu_families(),
        "write_families": crate::ecu_database::write_families(),
        "runtime_packs": crate::ecu_database::runtime_pack_count(),
        "protocols": ["auto","vpw","can","kwp","consult","uds"],
        "honest": true,
        "note": "Offline BIN/XDF works without an ECU. Live I/O needs an adapter you already own. Honda and P59 writes stay blocked. VerseLink PCB is parked."
    }).to_string())
}

#[tauri::command]
pub fn read_battery_voltage_cmd() -> Result<String, String> {
    if crate::j2534::is_device_open() {
        match crate::j2534::j2534_read_vbatt() {
            Ok(v) => return Ok(json!({"source":"j2534","volts":v,"ok": v >= 12.5 || v == 0.0}).to_string()),
            Err(e) => {
                return crate::with_port(|port| match crate::flash::read_battery_voltage(port) {
                    Some(v) => Ok(json!({"source":"pid42","volts":v,"ok": v >= 12.5, "j2534_note": e}).to_string()),
                    None => Err(format!("No battery voltage (J2534: {})", e)),
                });
            }
        }
    }
    crate::with_port(|port| match crate::flash::read_battery_voltage(port) {
        Some(v) => Ok(json!({"source":"pid42","volts":v,"ok": v >= 12.5}).to_string()),
        None => Err("No PID 0x42 / J2534 voltage".into()),
    })
}

#[tauri::command]
pub fn correct_bin_checksums_report(data: Vec<u8>) -> Result<String, String> {
    if crate::cs_guard::p01_corrector_blocked(&data) {
        let fam = if crate::cs_guard::p59_blocks_p01_corrector(&data) { "GM_P59" } else { "HONDA_KEIHIN" };
        return Ok(json!({"success": false, "error": format!("{} OS string. P01 additive correction is blocked.", fam)}).to_string());
    }
    match crate::checksum::correct_checksums(&data) {
        Ok(c) => Ok(json!({"success": true, "report": c.report, "bytes": c.data.len()}).to_string()),
        Err(e) => Ok(json!({"success": false, "error": e}).to_string()),
    }
}

#[tauri::command]
pub fn session_snapshot() -> Result<String, String> {
    let health = if crate::j2534::is_device_open() {
        "Connected (j2534)".to_string()
    } else if let Ok(g) = crate::STATE.lock() {
        if g.port.is_some() {
            format!("Connected ({})", g.protocol)
        } else {
            "Disconnected".into()
        }
    } else {
        "Disconnected".into()
    };
    let (os, family, proto) = if let Ok(g) = crate::STATE.lock() {
        (g.last_os_id.clone(), g.last_family.clone(), g.protocol.clone())
    } else {
        (None, None, String::new())
    };
    Ok(json!({
        "version": VERSION,
        "write_families": crate::ecu_database::write_families(),
        "health": health,
        "protocol": proto,
        "last_os_id": os,
        "last_family": family,
        "families": crate::ecu_database::list_supported_ecu_families(),
        "j2534_open": crate::j2534::is_device_open(),
        "runtime_packs": crate::ecu_database::runtime_pack_count(),
    }).to_string())
}

const ADAPTERS_JSON: &str = include_str!("../../reference/adapters/supported_adapters.json");

#[tauri::command]
pub fn list_supported_adapters() -> Result<String, String> {
    Ok(ADAPTERS_JSON.to_string())
}

#[tauri::command]
pub fn import_ecu_pack_cmd(json_text: String) -> Result<String, String> {
    let entry = crate::ecu_database::import_ecu_pack_json(&json_text)?;
    Ok(json!({
        "ok": true,
        "ecu_family": entry.ecu_family,
        "display_name": entry.display_name,
        "write_allowed": crate::ecu_database::write_path_live(&entry.ecu_family),
        "runtime_packs": crate::ecu_database::runtime_pack_count(),
        "note": "Runtime pack merged for this session. Write stays fail-closed unless the family is already in write_path_live."
    }).to_string())
}

#[tauri::command]
pub fn capabilities_matrix() -> Result<String, String> {
    Ok(json!({
        "version": VERSION,
        "offline": [
            {"id":"identify","status":"live","note":"OS string + size + catalog"},
            {"id":"checksum_p01","status":"live","note":"P01 additive; Honda/P59 blocked"},
            {"id":"checksum_edc16c41","status":"live","note":"Measured multipoint on 2MB EDC16C41"},
            {"id":"tableseek_p01","status":"live","note":"Universal Patcher pack on GM P01 dumps"},
            {"id":"xdf_a2l","status":"live","note":"Import + extract + patch + export XDF"},
            {"id":"map_from_log","status":"live","note":"Occupancy + STFT/LTFT blend preview"},
            {"id":"seedkey_gm_2byte","status":"live","note":"Public 2-byte tables. No 5-byte library."},
            {"id":"seedkey_edc16c41","status":"live","note":"4-byte algorithm with unit vectors"},
            {"id":"scripting","status":"live","note":"Deterministic bench language, no eval"}
        ],
        "hardware": [
            {"id":"elm_mode01","status":"adapter","note":"Live PIDs when ELM/STN answers"},
            {"id":"dtc","status":"adapter","note":"Mode 03/07/0A + freeze frame"},
            {"id":"j2534","status":"adapter","note":"Needs a vendor DLL on Windows"},
            {"id":"guided_flash","status":"limited","note":"P01_0411 and EDC16C41 only"}
        ],
        "blocked": [
            {"id":"honda_write","status":"blocked","note":"Keihin write stays closed"},
            {"id":"p59_write","status":"blocked","note":"Needs measured CS words + kernel"},
            {"id":"gm_5byte","status":"blocked","note":"Licensed library not shipped"},
            {"id":"verselink_pcb","status":"parked","note":"Software-first. Buy or reuse an adapter."}
        ],
        "write_families": crate::ecu_database::write_families(),
        "families": crate::ecu_database::list_supported_ecu_families()
    }).to_string())
}
