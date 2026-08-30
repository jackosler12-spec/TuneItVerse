//! v2.9.0 standalone tools: BIN identify, BIN diff, map-from-log.
//! Wired from lib.rs generate_handler (v3.0.0).

use serde_json::json;
use crate::logging;
use crate::ecu_database;

#[tauri::command]
pub fn identify_bin_cmd(data: Vec<u8>) -> Result<String, String> {
    Ok(identify_bin(&data).to_string())
}

#[tauri::command]
pub fn compare_bins_cmd(a: Vec<u8>, b: Vec<u8>) -> Result<String, String> {
    Ok(compare_bins(&a, &b).to_string())
}

#[tauri::command]
pub fn map_from_log_cmd() -> Result<String, String> {
    analyze_log().map(|v| v.to_string())
}

pub fn identify_bin(data: &[u8]) -> serde_json::Value {
    let size = data.len();
    let size_matches = ecu_database::list_ecus_by_bin_size(size);
    let family_by_size = size_matches.first().map(|e| e.ecu_family.clone());
    let display = size_matches.first().map(|e| e.display_name.clone());
    let families_same_size: Vec<String> = size_matches.iter().map(|e| e.ecu_family.clone()).collect();
    let hay = String::from_utf8_lossy(data);
    let mut hits = Vec::new();
    for fam in ecu_database::list_supported_ecu_families() {
        if let Some(e) = ecu_database::get_ecu_by_family(&fam) {
            for id in e.part_numbers_or_os_ids {
                if id.len() >= 5 && id.chars().any(|c| c.is_ascii_digit()) && hay.contains(&id) {
                    hits.push(json!({"os_or_part": id, "ecu_family": e.ecu_family, "display_name": e.display_name}));
                }
            }
        }
    }
    let mut extra = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i].is_ascii_digit() {
            let start = i;
            while i < data.len() && data[i].is_ascii_digit() { i += 1; }
            let n = i - start;
            if (7..=10).contains(&n) {
                extra.push(String::from_utf8_lossy(&data[start..i]).into_owned());
            }
        } else { i += 1; }
    }
    extra.sort();
    extra.dedup();
    extra.truncate(12);
    json!({
        "bin_size_bytes": size,
        "family_by_size": family_by_size,
        "families_same_size": families_same_size,
        "display_name": display,
        "os_hits_in_image": hits,
        "numeric_id_candidates": extra,
        "notes": if family_by_size.is_some() {
            "Size matched at least one family. Confirm OS ID — 512KB is P01 or P59; 2MB is EDC16/EDC17/MED17."
        } else {
            "Unknown size — add a JSON entry in reference/ecu_database/."
        }
    })
}

pub fn compare_bins(a: &[u8], b: &[u8]) -> serde_json::Value {
    if a.len() != b.len() {
        return json!({
            "same_size": false,
            "len_a": a.len(),
            "len_b": b.len(),
            "diff_bytes": serde_json::Value::Null,
            "first_diffs": [],
            "message": "Images are different lengths — cannot do a 1:1 compare."
        });
    }
    let mut diffs = 0usize;
    let mut first = Vec::new();
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            diffs += 1;
            if first.len() < 40 {
                first.push(json!({"offset": format!("0x{:06X}", i), "a": format!("{:02X}", x), "b": format!("{:02X}", y)}));
            }
        }
    }
    let pct = if a.is_empty() { 0.0 } else { (diffs as f64) * 100.0 / (a.len() as f64) };
    json!({
        "same_size": true,
        "len_a": a.len(),
        "len_b": b.len(),
        "diff_bytes": diffs,
        "same_bytes": a.len() - diffs,
        "diff_percent": (pct * 1000.0).round() / 1000.0,
        "identical": diffs == 0,
        "first_diffs": first,
        "message": if diffs == 0 { "Images are identical." } else { "Images differ — review first_diffs and re-run checksum after edits." }
    })
}

fn analyze_log() -> Result<serde_json::Value, String> {
    let samples = logging::get_samples(Some(5000));
    if samples.is_empty() {
        return Err("No log samples. Start a session and capture data first.".into());
    }
    let mut rpm_sum = 0.0; let mut n_rpm = 0.0;
    let mut map_sum = 0.0; let mut n_map = 0.0;
    let mut tps_sum = 0.0; let mut n_tps = 0.0;
    let mut rpm_min = f64::MAX; let mut rpm_max = f64::MIN;
    let mut map_min = f64::MAX; let mut map_max = f64::MIN;
    for s in &samples {
        if let Some(v) = s.values.get("rpm") { rpm_sum += v; n_rpm += 1.0; rpm_min = rpm_min.min(*v); rpm_max = rpm_max.max(*v); }
        if let Some(v) = s.values.get("map") { map_sum += v; n_map += 1.0; map_min = map_min.min(*v); map_max = map_max.max(*v); }
        if let Some(v) = s.values.get("tps") { tps_sum += v; n_tps += 1.0; }
    }
    let rpm_avg = if n_rpm > 0.0 { rpm_sum / n_rpm } else { 0.0 };
    let map_avg = if n_map > 0.0 { map_sum / n_map } else { 0.0 };
    let tps_avg = if n_tps > 0.0 { tps_sum / n_tps } else { 0.0 };
    let rpm_cell = ((rpm_avg / 500.0).floor() as i32).clamp(0, 15);
    let map_cell = ((map_avg / 16.0).floor() as i32).clamp(0, 15);
    Ok(json!({
        "sample_count": samples.len(),
        "rpm_avg": (rpm_avg * 10.0).round() / 10.0,
        "rpm_min": if n_rpm > 0.0 { (rpm_min * 10.0).round() / 10.0 } else { 0.0 },
        "rpm_max": if n_rpm > 0.0 { (rpm_max * 10.0).round() / 10.0 } else { 0.0 },
        "map_avg_kpa": (map_avg * 10.0).round() / 10.0,
        "map_min_kpa": if n_map > 0.0 { (map_min * 10.0).round() / 10.0 } else { 0.0 },
        "map_max_kpa": if n_map > 0.0 { (map_max * 10.0).round() / 10.0 } else { 0.0 },
        "tps_avg": (tps_avg * 10.0).round() / 10.0,
        "suggested_ve_cell": {"row_rpm": rpm_cell, "col_map": map_cell},
        "advice": format!("Session spent most time near {:.0} RPM / {:.0} kPa MAP (hint cell r{} c{}). Tune that region first. Hint only — not auto-write.", rpm_avg, map_avg, rpm_cell, map_cell)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_unknown_size() {
        let v = identify_bin(&[0u8; 64]);
        assert_eq!(v["bin_size_bytes"], 64);
        assert!(v["family_by_size"].is_null());
    }

    #[test]
    fn identify_p01_size() {
        let v = identify_bin(&vec![0u8; 524288]);
        assert_eq!(v["bin_size_bytes"], 524288);
        assert_eq!(v["family_by_size"], "P01_0411");
    }

    #[test]
    fn compare_same() {
        let a = vec![1u8, 2, 3, 4];
        let v = compare_bins(&a, &a);
        assert_eq!(v["identical"], true);
        assert_eq!(v["diff_bytes"], 0);
    }

    #[test]
    fn compare_diff_len() {
        let v = compare_bins(&[1, 2], &[1, 2, 3]);
        assert_eq!(v["same_size"], false);
    }

    #[test]
    fn compare_first_diff() {
        let a = vec![0u8; 8];
        let mut b = a.clone();
        b[3] = 0xFF;
        let v = compare_bins(&a, &b);
        assert_eq!(v["diff_bytes"], 1);
        assert_eq!(v["identical"], false);
    }
}
