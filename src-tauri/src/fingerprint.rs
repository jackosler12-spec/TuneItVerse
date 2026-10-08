//! Offline BIN fingerprint and honest capability coverage.
//!
//! Matching a catalog string does not enable write.

use serde_json::{json, Value};

const MARKERS: &[(&str, &str)] = &[
    ("EDC16", "EDC16C41"),
    ("EDC17", "EDC17_COMMON"),
    ("MED17", "MED17_COMMON"),
    ("ME7", "ME7_COMMON"),
    ("SID803", "SIEMENS_SID803"),
    ("SIMOS", "SIMOS18"),
    ("TRIONIC", "TRIONIC8"),
    ("TRANSTRON", "TRANSTRON_4HK1"),
    ("12225074", "P01_0411"),
    ("0411", "P01_0411"),
];

pub fn checksum_impl(family: &str) -> &'static str {
    match family.to_ascii_uppercase().as_str() {
        "P01_0411" => "implemented_additive16",
        "EDC16C41" => "implemented_crc32_multipoint",
        "GM_P59" => "blocked_until_measured_words",
        "HONDA_KEIHIN" => "blocked_do_not_use_p01",
        _ => "catalog_only",
    }
}

fn ascii_upper(data: &[u8]) -> String {
    data.iter()
        .map(|b| if (0x20..=0x7E).contains(b) { (*b as char).to_ascii_uppercase() } else { ' ' })
        .collect()
}

pub fn fingerprint(data: &[u8]) -> Value {
    let text = ascii_upper(data);
    let mut hits = Vec::new();
    for fam in crate::ecu_database::list_supported_ecu_families() {
        let Some(entry) = crate::ecu_database::get_ecu_by_family(&fam) else { continue };
        let mut reasons = Vec::new();
        let mut score = 0i32;
        if entry.bin_size_bytes as usize == data.len() {
            score += 2;
            reasons.push("size".to_string());
        }
        for raw in &entry.part_numbers_or_os_ids {
            let token = raw.trim().to_ascii_uppercase();
            if token.len() >= 5 && text.contains(&token) {
                score += 5;
                reasons.push(format!("part:{token}"));
            }
        }
        for (marker, mapped) in MARKERS {
            if *mapped == entry.ecu_family && text.contains(marker) {
                score += 3;
                reasons.push(format!("marker:{marker}"));
            }
        }
        if score > 0 {
            hits.push(json!({
                "family": entry.ecu_family,
                "display_name": entry.display_name,
                "score": score,
                "reasons": reasons,
                "checksum_impl": checksum_impl(&entry.ecu_family),
                "catalog_write_path": crate::ecu_database::write_path_live(&entry.ecu_family),
                "write_allowed": false,
            }));
        }
    }
    hits.sort_by(|a, b| b["score"].as_i64().unwrap_or(0).cmp(&a["score"].as_i64().unwrap_or(0)));
    hits.truncate(8);
    json!({
        "bin_size_bytes": data.len(),
        "hits": hits,
        "write_allowed": false,
        "notes": "Fingerprint is offline. It does not unlock or enable write.",
    })
}

pub fn coverage() -> Value {
    let families = crate::ecu_database::load_ecu_database();
    let rows: Vec<Value> = families.iter().map(|e| {
        json!({
            "family": e.ecu_family,
            "display_name": e.display_name,
            "protocol": e.protocol,
            "bin_size_bytes": e.bin_size_bytes,
            "checksum_catalog": e.checksum.r#type,
            "checksum_impl": checksum_impl(&e.ecu_family),
            "security_catalog": e.security_access.r#type,
            "catalog_write_path": crate::ecu_database::write_path_live(&e.ecu_family),
            "write_allowed": false,
        })
    }).collect();
    json!({
        "version": crate::APP_VERSION,
        "write_families": crate::ecu_database::write_families(),
        "rows": rows,
        "notes": "catalog_only means identify/hints. It is not a flash path.",
    })
}

#[tauri::command]
pub fn fingerprint_bin_cmd(data: Vec<u8>) -> Result<String, String> {
    serde_json::to_string(&fingerprint(&data)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn coverage_report_cmd() -> Result<String, String> {
    serde_json::to_string(&coverage()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p01_marker_scores_without_enabling_write() {
        let mut bin = vec![0u8; 64];
        bin.extend(b"OS 12225074 HOLDEN");
        let report = fingerprint(&bin);
        assert_eq!(report["write_allowed"], false);
        let hits = report["hits"].as_array().unwrap();
        assert!(hits.iter().any(|h| h["family"] == "P01_0411"));
        assert!(hits.iter().all(|h| h["write_allowed"] == false));
    }

    #[test]
    fn coverage_blocks_p59_and_honda() {
        let report = coverage();
        let rows = report["rows"].as_array().unwrap();
        assert!(rows.len() >= 10);
        assert!(rows.iter().all(|r| r["write_allowed"] == false));
        assert!(rows.iter().any(|r| r["family"] == "GM_P59" && r["checksum_impl"] == "blocked_until_measured_words"));
        assert!(rows.iter().any(|r| r["family"] == "HONDA_KEIHIN" && r["catalog_write_path"] == false));
    }
}
