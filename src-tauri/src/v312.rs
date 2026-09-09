//! v3.12.0 command extras: app_info, battery voltage, checksum report.
use serde_json::json;

#[tauri::command]
pub fn app_info() -> Result<String, String> {
    Ok(json!({
        "name": "TuneItVerse",
        "version": "3.12.0",
        "families": crate::ecu_database::list_supported_ecu_families(),
        "protocols": ["auto","vpw","can","kwp","consult","uds"],
        "honest": true,
        "note": "Offline BIN/XDF works without an ECU. Live I/O needs an adapter."
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
    match crate::checksum::correct_checksums(&data) {
        Ok(c) => Ok(json!({"success": true, "report": c.report, "bytes": c.data.len()}).to_string()),
        Err(e) => Ok(json!({"success": false, "error": e}).to_string()),
    }
}
