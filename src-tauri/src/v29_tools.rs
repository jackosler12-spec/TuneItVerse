//! v2.9.0 helpers: BIN identify, BIN diff, map-from-log glue.
//! Also re-exports size constants used by checksum dispatch notes.

use serde_json::json;
use crate::ecu_database;
use crate::logging;

#[tauri::command]
pub fn identify_bin_cmd(data: Vec<u8>) -> Result<String, String> {
    Ok(ecu_database::identify_from_bin(&data).to_string())
}

#[tauri::command]
pub fn compare_bins_cmd(a: Vec<u8>, b: Vec<u8>) -> Result<String, String> {
    Ok(ecu_database::compare_bin_images(&a, &b).to_string())
}

#[tauri::command]
pub fn map_from_log_cmd() -> Result<String, String> {
    logging::analyze_session_for_maps().map(|v| v.to_string())
}

/// Fallback identify if ecu_database helpers are not yet linked.
#[allow(dead_code)]
pub fn identify_bin_fallback(data: &[u8]) -> serde_json::Value {
    json!({
        "bin_size_bytes": data.len(),
        "family_by_size": match data.len() {
            524288 => Some("P01_0411"),
            131072 => Some("P01_0411"),
            2097152 => Some("EDC16C41"),
            _ => None,
        },
        "notes": "Size fingerprint only (fallback)."
    })
}
