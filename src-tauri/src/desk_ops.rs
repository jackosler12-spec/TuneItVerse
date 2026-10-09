//! Offline desk operations. No ECU traffic. Does not enable write.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize)]
pub struct TableCsvRequest {
    pub values: Vec<Vec<f64>>,
    pub row_labels: Option<Vec<String>>,
    pub col_labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableCsvResult {
    pub csv: String,
    pub rows: usize,
    pub cols: usize,
    pub write_allowed: bool,
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn table_csv(req: &TableCsvRequest) -> Result<TableCsvResult, String> {
    if req.values.is_empty() || req.values[0].is_empty() {
        return Err("empty table".into());
    }
    let cols = req.values[0].len();
    if req.values.iter().any(|r| r.len() != cols) {
        return Err("ragged table".into());
    }
    let row_labels = req.row_labels.clone().unwrap_or_default();
    let col_labels = req.col_labels.clone().unwrap_or_default();
    let mut csv = String::new();
    csv.push_str("row");
    for c in 0..cols {
        csv.push(',');
        if c < col_labels.len() {
            csv.push_str(&csv_escape(&col_labels[c]));
        } else {
            csv.push_str(&format!("c{}", c));
        }
    }
    csv.push('\n');
    for (r, row) in req.values.iter().enumerate() {
        if r < row_labels.len() {
            csv.push_str(&csv_escape(&row_labels[r]));
        } else {
            csv.push_str(&format!("r{}", r));
        }
        for cell in row {
            csv.push(',');
            if cell.is_finite() {
                csv.push_str(&format!("{}", cell));
            } else {
                csv.push_str("NaN");
            }
        }
        csv.push('\n');
    }
    Ok(TableCsvResult {
        csv,
        rows: req.values.len(),
        cols,
        write_allowed: false,
    })
}

pub fn seed_coverage() -> Value {
    let tables = crate::seed_tables::load_tables();
    let families = crate::ecu_database::list_supported_ecu_families();
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for pair in &tables.pairs {
        *counts.entry(pair.family.clone()).or_insert(0) += 1;
    }
    let missing: Vec<String> = families
        .iter()
        .filter(|fam| !counts.keys().any(|k| k.eq_ignore_ascii_case(fam)))
        .cloned()
        .collect();
    json!({
        "version": crate::APP_VERSION,
        "pair_count": tables.pairs.len(),
        "families_with_pairs": counts,
        "catalog_missing_pairs": missing,
        "write_allowed": false,
        "notes": "Empty families stay fail-closed. Put seed/key pairs from your own dumps in reference/ecu_database/seed_tables.json."
    })
}

pub fn flash_readiness(data: &[u8]) -> Value {
    let ident = crate::v29_tools::identify_bin(data);
    let checksum = crate::checksum::validate_checksums(data).ok();
    let family = ident.get("family").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let write_allowed = ident.get("write_allowed").and_then(|v| v.as_bool()).unwrap_or(false)
        && crate::ecu_database::write_path_live(&family);
    let checksum_ok = checksum.as_ref().map(|r| r.all_valid).unwrap_or(false);
    let mut blockers: Vec<String> = Vec::new();
    if data.is_empty() {
        blockers.push("No BIN loaded".into());
    }
    if family.is_empty() {
        blockers.push("Family unresolved. Do not write.".into());
    }
    if !write_allowed {
        blockers.push("Write path is not live for this image. P01_0411 and EDC16C41 only.".into());
    }
    if !checksum_ok {
        blockers.push("Checksum report is not all-valid. Correct only with a verified routine.".into());
    }
    blockers.push("Guided flash still requires a FullImage backup.".into());
    blockers.push("Battery must read at least 12.5 V before any write.".into());
    let offline_image_ok = !data.is_empty() && !family.is_empty() && write_allowed && checksum_ok;
    json!({
        "version": crate::APP_VERSION,
        "family": if family.is_empty() { Value::Null } else { json!(family) },
        "display_name": ident.get("display_name").cloned().unwrap_or(Value::Null),
        "bin_size_bytes": data.len(),
        "write_allowed": write_allowed,
        "checksum_all_valid": checksum_ok,
        "checksum_method": checksum.as_ref().map(|r| r.method_used.clone()).unwrap_or_default(),
        "offline_image_ok": offline_image_ok,
        "flash_allowed_now": false,
        "blockers": blockers,
        "notes": "offline_image_ok is not a flash authorization. Voltage, backup, unlock, and live verify still gate the write path.",
        "sha256": ident.get("sha256").cloned().unwrap_or(Value::Null)
    })
}

pub fn self_check_ok() -> bool {
    let empty = flash_readiness(&[]);
    if empty.get("flash_allowed_now").and_then(|v| v.as_bool()) != Some(false) {
        return false;
    }
    if empty.get("offline_image_ok").and_then(|v| v.as_bool()) != Some(false) {
        return false;
    }
    let csv = table_csv(&TableCsvRequest {
        values: vec![vec![1.5, 2.0]],
        row_labels: Some(vec!["rpm".into()]),
        col_labels: Some(vec!["a".into(), "b".into()]),
    });
    csv.map(|r| r.csv.contains("1.5") && !r.write_allowed).unwrap_or(false)
}

#[tauri::command]
pub fn flash_readiness_cmd(data: Vec<u8>) -> Result<String, String> {
    Ok(flash_readiness(&data).to_string())
}

#[tauri::command]
pub fn seed_coverage_cmd() -> Result<String, String> {
    Ok(seed_coverage().to_string())
}

#[tauri::command]
pub fn table_csv_cmd(req: TableCsvRequest) -> Result<TableCsvResult, String> {
    table_csv(&req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_image_is_not_a_flash_auth() {
        let r = flash_readiness(&[]);
        assert_eq!(r["flash_allowed_now"], false);
        assert_eq!(r["offline_image_ok"], false);
        assert_eq!(r["write_allowed"], false);
    }

    #[test]
    fn unknown_size_stays_blocked() {
        let r = flash_readiness(&[0x11, 0x22, 0x33, 0x44]);
        assert_eq!(r["write_allowed"], false);
        assert_eq!(r["flash_allowed_now"], false);
    }

    #[test]
    fn csv_round_trip_shape() {
        let r = table_csv(&TableCsvRequest {
            values: vec![vec![1.0, 2.5], vec![3.0, 4.0]],
            row_labels: None,
            col_labels: Some(vec!["low".into(), "high".into()]),
        })
        .unwrap();
        assert_eq!(r.rows, 2);
        assert!(r.csv.lines().count() == 3);
        assert!(!r.write_allowed);
    }

    #[test]
    fn coverage_does_not_enable_write() {
        let r = seed_coverage();
        assert_eq!(r["write_allowed"], false);
        assert!(r["catalog_missing_pairs"].as_array().is_some());
    }
}
