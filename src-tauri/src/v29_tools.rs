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
    Ok(json!({"tool":"TuneItVerse","version":"3.27.0","families":ecu_database::list_supported_ecu_families(),"identify":ident,"map_from_log":log}).to_string())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

fn printable_strings(data: &[u8], min_len: usize, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for &b in data {
        if (0x20..=0x7E).contains(&b) {
            cur.push(b as char);
        } else {
            if cur.len() >= min_len && out.len() < limit {
                out.push(cur.clone());
            }
            cur.clear();
        }
    }
    if cur.len() >= min_len && out.len() < limit {
        out.push(cur);
    }
    out
}

fn os_id_tokens(raw: &str) -> Vec<String> {
    raw.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 5 && t.len() <= 16 && t.chars().any(|c| c.is_ascii_digit()))
        .map(|t| t.to_ascii_uppercase())
        .collect()
}

fn match_os_ids(strings: &[String]) -> Vec<serde_json::Value> {
    let hay: Vec<String> = strings.iter().map(|s| s.to_ascii_uppercase()).collect();
    let mut hits = Vec::new();
    for fam in ecu_database::list_supported_ecu_families() {
        if let Some(e) = ecu_database::get_ecu_by_family(&fam) {
            for raw in &e.part_numbers_or_os_ids {
                for tok in os_id_tokens(raw) {
                    if hay.iter().any(|s| s.contains(&tok)) {
                        hits.push(json!({"family": e.ecu_family, "matched": tok, "display_name": e.display_name}));
                    }
                }
            }
        }
    }
    hits
}

pub fn identify_bin(data: &[u8]) -> serde_json::Value {
    let size = data.len();
    let mut size_matches = Vec::new();
    for fam in ecu_database::list_supported_ecu_families() {
        if let Some(e) = ecu_database::get_ecu_by_family(&fam) {
            if e.bin_size_bytes as usize == size { size_matches.push(e); }
        }
    }
    let strings = printable_strings(data, 5, 64);
    let os_hits = match_os_ids(&strings);
    let family_by_os = os_hits.first().and_then(|h| h.get("family").and_then(|v| v.as_str()).map(|s| s.to_string()));
    let family_by_size = size_matches.first().map(|e| e.ecu_family.clone());
    let honda_os = crate::cs_guard::looks_like_honda(data);
    let gm_p01_os = crate::cs_guard::looks_like_gm_p01(data);
    let gm_p59_os = crate::cs_guard::looks_like_gm_p59(data);
    let size_collision = size_matches.len() > 1;
    let family = if honda_os && !gm_p01_os {
        Some("HONDA_KEIHIN".to_string())
    } else if gm_p59_os && !gm_p01_os {
        Some("GM_P59".to_string())
    } else if let Some(os) = family_by_os.clone() {
        Some(os)
    } else if size_collision {
        None
    } else {
        family_by_size.clone()
    };
    // Only families with a measured corrector AND a live write path.
    let correction_safe = match family.as_deref() {
        Some("P01_0411") if !honda_os && !gm_p59_os => true,
        Some("EDC16C41") if size == crate::checksum_sizes::EDC16_FLASH_SIZE => true,
        _ => false,
    };
    let write_allowed = matches!(family.as_deref(), Some("P01_0411") | Some("EDC16C41")) && correction_safe;
    let display = family.as_ref().and_then(|f| ecu_database::get_ecu_by_family(f).map(|e| e.display_name));
    let families_same_size: Vec<String> = size_matches.iter().map(|e| e.ecu_family.clone()).collect();
    let head_n = size.min(4096);
    let tail_n = size.min(4096);
    let tail = if size > 0 { &data[size - tail_n..] } else { data };
    json!({
        "bin_size_bytes": size,
        "family": family,
        "family_by_os": family_by_os,
        "family_by_size": family_by_size,
        "families_same_size": families_same_size,
        "os_id_hits": os_hits,
        "display_name": display,
        "sha256": sha256_hex(data),
        "sha256_head_4k": sha256_hex(&data[..head_n]),
        "sha256_tail_4k": sha256_hex(tail),
        "printable_strings": strings.into_iter().take(24).collect::<Vec<_>>(),
        "honda_os": honda_os,
        "gm_p01_os": gm_p01_os,
        "gm_p59_os": gm_p59_os,
        "size_collision": size_collision,
        "correction_safe": correction_safe,
        "write_allowed": write_allowed,
        "notes": if honda_os && !gm_p01_os {
            "Honda OS string. P01 additive correction is blocked."
        } else if gm_p59_os && !gm_p01_os {
            "P59 OS string. P01 additive and write stay blocked until measured P59 CS words exist."
        } else if family_by_os.is_some() {
            "OS/part string matched the ECU catalog. Confirm the dump is yours before write."
        } else if size_collision {
            "Size collides across catalog families. Confirm OS string before any corrector."
        } else if family_by_size.is_some() {
            "Size matched one family. Confirm OS ID against strings."
        } else {
            "Unknown size — add JSON in reference/ecu_database/."
        }
    })
}

/// Fail-closed family for write/compare. Size collision or Honda-without-GM stays an error.
pub fn resolved_family(data: &[u8]) -> Result<String, String> {
    let v = identify_bin(data);
    if v.get("honda_os").and_then(|x| x.as_bool()).unwrap_or(false)
        && !v.get("gm_p01_os").and_then(|x| x.as_bool()).unwrap_or(false)
    {
        return Err("Honda OS string. Write/compare refused.".into());
    }
    if v.get("gm_p59_os").and_then(|x| x.as_bool()).unwrap_or(false)
        && !v.get("gm_p01_os").and_then(|x| x.as_bool()).unwrap_or(false)
    {
        return Err("P59 OS string. Write/compare refused until measured P59 CS words and kernel exist.".into());
    }
    if let Some(f) = v.get("family").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
        if f.eq_ignore_ascii_case("GM_P59") || f.to_ascii_uppercase().contains("P59") {
            return Err("GM_P59 write/compare refused. No measured checksum words or kernel.".into());
        }
        return Ok(f.to_string());
    }
    Err(v.get("notes").and_then(|x| x.as_str()).unwrap_or("ECU family unresolved").to_string())
}
