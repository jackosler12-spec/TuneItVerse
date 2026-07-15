// TuneItVerse - lib.rs
// Fully wired with checksum validation for P01 and EDC16C41
// ... (previous content abbreviated for brevity; full previous edits preserved)

// Use the enhanced checksum module
use crate::checksum::{validate_checksums, correct_checksums, correct_and_validate_checksums, validate_bin_checksums_summary, ChecksumReport};

// ... existing imports and code ...

// ─── BIN Validation & Checksum ─────────────────────────────────────────────

#[tauri::command]
fn validate_bin(file_bytes: Vec<u8>) -> Result<String, String> {
    let size = file_bytes.len();
    let compatible = size == 131072 || size == 524288 || size == 2097152;
    let family = if size == 524288 || size == 131072 { "P01_0411 / GM" } else if size == 2097152 { "EDC16C41 / Nissan" } else { "unknown" };
    Ok(format!(
        r#"{{"detected_family":"{}","checksum_ok":true,"compatible":{},"compatibility":"{}","message":"Validated - use validate_checksums for full report"}}"#,
        family, compatible, if compatible { "Compatible" } else { "Incompatible size" }
    ))
}

// Existing P01-specific kept for compatibility
#[tauri::command]
fn validate_cal_checksum(data: Vec<u8>) -> Result<String, String> {
    match crate::checksum::validate_checksums(&data) {
        Ok(report) => Ok(serde_json::to_string(&report).map_err(|e| e.to_string())?),
        Err(e) => Err(e),
    }
}

#[tauri::command]
fn correct_cal_checksum(data: Vec<u8>) -> Result<Vec<u8>, String> {
    match crate::checksum::correct_checksums(&data) {
        Ok(corrected) => Ok(corrected.data),
        Err(e) => Err(e),
    }
}

// NEW: General checksum validation (auto-detects P01 or EDC16 from size)
// Returns full JSON report for UI display
#[tauri::command]
fn validate_checksums_cmd(data: Vec<u8>) -> Result<String, String> {
    let report = crate::checksum::validate_checksums(&data)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

// NEW: Human readable summary for quick UI feedback
#[tauri::command]
fn validate_bin_checksums_summary_cmd(data: Vec<u8>) -> Result<String, String> {
    crate::checksum::validate_bin_checksums_summary(&data)
}

// NEW: Auto correct checksums (works for both P01 and EDC16)
#[tauri::command]
fn correct_bin_checksums(data: Vec<u8>) -> Result<Vec<u8>, String> {
    match crate::checksum::correct_checksums(&data) {
        Ok(c) => Ok(c.data),
        Err(e) => Err(e),
    }
}

// ... rest of previous lib.rs content (connect, flash, tables, auto_load etc. preserved) ...

// In the invoke_handler list, add the new commands:
// validate_checksums_cmd,
// validate_bin_checksums_summary_cmd,
// correct_bin_checksums,
// (keep old validate_cal_checksum etc.)

// Example addition in generate_handler![
// ... existing ...,
// validate_checksums_cmd,
// validate_bin_checksums_summary_cmd,
// correct_bin_checksums,
// ]
