//! command extras: app_info, battery voltage, checksum report, session snapshot.
use serde_json::json;

#[tauri::command]
pub fn app_info() -> Result<String, String> {
    Ok(json!({
        "name": "TuneItVerse",
        "version": crate::APP_VERSION,
        "families": crate::ecu_database::list_supported_ecu_families(),
        "write_families": crate::ecu_database::write_families(),
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
                return crate::with_port(|port| match crate::flash::read_battery_voltage_src(port) {
                    Some((v, src)) => Ok(voltage_json(src, v, Some(e)).to_string()),
                    None => Err(format!("No battery voltage (J2534: {})", e)),
                });
            }
        }
    }
    crate::with_port(|port| match crate::flash::read_battery_voltage_src(port) {
        Some((v, src)) => Ok(voltage_json(src, v, None).to_string()),
        None => Err("No supply voltage: adapter gave no STVR/ATRV and the ECU does not list PID 0x42 (or did not answer it).".into()),
    })
}

/// JSON for a voltage reading. `source` names the real origin of the number:
/// adapter_stvr / adapter_atrv (adapter pin 16) or pid42 / pid42_vpw_raw (ECU).
pub fn voltage_json(source: &str, volts: f32, j2534_note: Option<String>) -> serde_json::Value {
    let mut v = json!({"source": source, "volts": volts, "ok": volts >= 12.5});
    if let Some(n) = j2534_note {
        v["j2534_note"] = json!(n);
    }
    v
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
        "version": crate::APP_VERSION,
        "write_families": crate::ecu_database::write_families(),
        "health": health,
        "protocol": proto,
        "last_os_id": os,
        "last_family": family,
        "families": crate::ecu_database::list_supported_ecu_families(),
        "j2534_open": crate::j2534::is_device_open(),
    }).to_string())
}

const ADAPTERS_JSON: &str = include_str!("../../reference/adapters/supported_adapters.json");

#[tauri::command]
pub fn list_supported_adapters() -> Result<String, String> {
    Ok(ADAPTERS_JSON.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voltage_label_is_the_real_source() {
        let v = voltage_json(crate::transport::VoltageSource::AdapterAtrv.label(), 13.8, None);
        assert_eq!(v["source"], "adapter_atrv");
        assert_ne!(v["source"], "pid42");
        let v = voltage_json(crate::transport::VoltageSource::AdapterStvr.label(), 12.4, Some("no vbatt".into()));
        assert_eq!(v["source"], "adapter_stvr");
        assert_eq!(v["ok"], false);
        assert_eq!(v["j2534_note"], "no vbatt");
    }
}
