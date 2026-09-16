//! BIN identify, BIN diff, map-from-log occupancy heatmap, workspace export.
use serde_json::json;
use sha2::{Digest, Sha256};
use crate::logging;
use crate::ecu_database;

#[tauri::command]
pub fn identify_bin_cmd(data: Vec<u8>) -> Result<String, String> { Ok(identify_bin(&data).to_string()) }
#[tauri::command]
pub fn compare_bins_cmd(a: Vec<u8>, b: Vec<u8>) -> Result<String, String> { Ok(compare_bins(&a, &b).to_string()) }
#[tauri::command]
pub fn map_from_log_cmd(csv: Option<String>) -> Result<String, String> {
    if let Some(raw) = csv {
        if !raw.trim().is_empty() {
            let _ = logging::import_csv(&raw)?;
        }
    }
    analyze_log().map(|v| v.to_string())
}
#[tauri::command]
pub fn export_workspace_cmd(data: Option<Vec<u8>>) -> Result<String, String> {
    let ident = data.as_deref().map(identify_bin);
    let log = analyze_log().unwrap_or_else(|e| json!({"error": e}));
    Ok(json!({"tool":"TuneItVerse","version":"3.21.0","families":ecu_database::list_supported_ecu_families(),"identify":ident,"map_from_log":log}).to_string())
}
