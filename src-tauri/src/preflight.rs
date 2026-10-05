//! Offline flash preflight. Does not talk to an ECU and cannot enable write.
//! Ready only when identify already says the image is a live write family
//! and the measured checksum routine reports valid.

use serde_json::{json, Value};

use crate::checksum;
use crate::v29_tools;

pub fn flash_preflight(data: &[u8]) -> Value {
    let id = v29_tools::identify_bin(data);
    let family = id
        .get("family")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let write_allowed = id.get("write_allowed").and_then(|v| v.as_bool()).unwrap_or(false);
    let correction_safe = id.get("correction_safe").and_then(|v| v.as_bool()).unwrap_or(false);
    let size = data.len();
    let cs = checksum::validate_checksums(data).ok();
    let checksum_valid = cs.as_ref().map(|r| r.all_valid).unwrap_or(false);
    let method = cs
        .as_ref()
        .map(|r| r.method_used.clone())
        .unwrap_or_else(|| "unavailable".into());
    let expected = if family.is_empty() {
        None
    } else {
        crate::ecu_database::get_ecu_by_family(&family).map(|e| e.bin_size_bytes as usize)
    };
    let size_ok = expected.map(|n| n == size).unwrap_or(false);
    let mut blockers: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    if data.is_empty() {
        blockers.push("No BIN loaded. Load a personal dump on Maps first.".into());
    }
    if family.is_empty() {
        blockers.push("Family not resolved. Size collision or unknown image — confirm the OS string.".into());
    }
    if !write_allowed {
        blockers.push("Write path is not live for this image. Only P01_0411 and EDC16C41 are write-enabled.".into());
    }
    if !checksum_valid {
        blockers.push(format!(
            "Checksum not valid ({method}). Correct with the measured routine, then re-run preflight."
        ));
    }
    if !family.is_empty() && !size_ok {
        blockers.push("Image size does not match the catalog entry.".into());
    }
    warnings.push("Voltage, backup quality, and live verify still need a connected adapter. This check is offline.".into());
    warnings.push("Personal dumps only. Never flash without a verified backup and stable power.".into());
    let ready = data.len() > 0
        && blockers.is_empty()
        && write_allowed
        && checksum_valid
        && correction_safe
        && size_ok;
    json!({
        "app_version": crate::APP_VERSION,
        "bytes": size,
        "family": if family.is_empty() { Value::Null } else { json!(family) },
        "display_name": id.get("display_name").cloned().unwrap_or(Value::Null),
        "os_id_hits": id.get("os_id_hits").cloned().unwrap_or(json!([])),
        "write_allowed": write_allowed,
        "correction_safe": correction_safe,
        "checksum_valid": checksum_valid,
        "checksum_method": method,
        "checksum_regions": cs.as_ref().map(|r| r.regions.len()).unwrap_or(0),
        "size_matches_catalog": size_ok,
        "expected_bytes": expected,
        "blockers": blockers,
        "warnings": warnings,
        "ready_for_guided_flash": ready,
        "write_families": ["P01_0411", "EDC16C41"],
        "notes": "Preflight does not unlock, write, or talk to the ECU."
    })
}

pub fn self_check_ok() -> bool {
    let empty = flash_preflight(&[]);
    empty.get("ready_for_guided_flash").and_then(|v| v.as_bool()) == Some(false)
        && empty
            .get("blockers")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
}

#[tauri::command]
pub fn flash_preflight_cmd(data: Option<Vec<u8>>) -> Result<String, String> {
    Ok(flash_preflight(data.as_deref().unwrap_or(&[])).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_not_ready() {
        let v = flash_preflight(&[]);
        assert_eq!(v["ready_for_guided_flash"], false);
        assert!(v["blockers"].as_array().unwrap().len() >= 1);
        assert!(self_check_ok());
    }

    #[test]
    fn blank_512k_is_not_a_write() {
        let v = flash_preflight(&vec![0u8; 524288]);
        assert_eq!(v["write_allowed"], false);
        assert_eq!(v["ready_for_guided_flash"], false);
    }
}
