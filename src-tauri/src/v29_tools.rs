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
pub fn map_from_log_cmd() -> Result<String, String> { analyze_log().map(|v| v.to_string()) }
#[tauri::command]
pub fn export_workspace_cmd(data: Option<Vec<u8>>) -> Result<String, String> {
    let ident = data.as_deref().map(identify_bin);
    let log = analyze_log().unwrap_or_else(|e| json!({"error": e}));
    Ok(json!({"tool":"TuneItVerse","version":"3.10.1","families":ecu_database::list_supported_ecu_families(),"identify":ident,"map_from_log":log}).to_string())
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
    let size_collision = size_matches.len() > 1;
    let family = if honda_os && !gm_p01_os {
        Some("HONDA_KEIHIN".to_string())
    } else if let Some(os) = family_by_os.clone() {
        Some(os)
    } else if size_collision {
        None
    } else {
        family_by_size.clone()
    };
    let correction_safe = match family.as_deref() {
        Some("P01_0411") | Some("GM_P59") if !honda_os => true,
        Some("EDC16C41") if size == crate::checksum_sizes::EDC16_FLASH_SIZE => true,
        _ => false,
    };
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
        "size_collision": size_collision,
        "correction_safe": correction_safe,
        "notes": if honda_os && !gm_p01_os {
            "Honda OS string. P01 additive correction is blocked."
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
    if let Some(f) = v.get("family").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
        return Ok(f.to_string());
    }
    Err(v.get("notes").and_then(|x| x.as_str()).unwrap_or("ECU family unresolved").to_string())
}

#[tauri::command]
pub fn import_workspace_cmd(json_text: String) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("invalid workspace JSON: {}", e))?;
    Ok(json!({
        "accepted": true,
        "tool": v.get("tool").and_then(|x| x.as_str()).unwrap_or(""),
        "version": v.get("version").and_then(|x| x.as_str()).unwrap_or("unknown"),
        "has_identify": v.get("identify").is_some(),
        "has_map_from_log": v.get("map_from_log").is_some(),
        "identify": v.get("identify"),
        "map_from_log": v.get("map_from_log"),
        "families": v.get("families"),
        "note": "Workspace metadata imported. BIN bytes are not restored from JSON — load the dump separately."
    }).to_string())
}

#[tauri::command]
pub fn patch_bin_bytes_cmd(data: Vec<u8>, offset: u32, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.is_empty() { return Err("no bytes to poke".into()); }
    let start = offset as usize;
    if start.checked_add(bytes.len()).map(|end| end > data.len()).unwrap_or(true) {
        return Err(format!("poke 0x{:X}+{} outside image ({} bytes)", offset, bytes.len(), data.len()));
    }
    let mut out = data;
    out[start..start+bytes.len()].copy_from_slice(&bytes);
    Ok(out)
}

pub fn compare_bins(a: &[u8], b: &[u8]) -> serde_json::Value {
    if a.len() != b.len() {
        return json!({"same_size":false,"len_a":a.len(),"len_b":b.len(),"diff_bytes":serde_json::Value::Null,"first_diffs":[],"diff_ranges":[],"message":"Images are different lengths."});
    }
    let mut diffs = 0usize;
    let mut first = Vec::new();
    let mut ranges = Vec::new();
    let mut range_start: Option<usize> = None;
    for (i,(x,y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            diffs += 1;
            if first.len() < 40 {
                first.push(json!({"offset":format!("0x{:06X}", i),"a":format!("{:02X}", x),"b":format!("{:02X}", y)}));
            }
            if range_start.is_none() { range_start = Some(i); }
        } else if let Some(s) = range_start.take() {
            ranges.push(json!({"start":format!("0x{:06X}", s),"end":format!("0x{:06X}", i-1),"length": i - s}));
        }
    }
    if let Some(s) = range_start {
        ranges.push(json!({"start":format!("0x{:06X}", s),"end":format!("0x{:06X}", a.len().saturating_sub(1)),"length": a.len() - s}));
    }
    let pct = if a.is_empty() { 0.0 } else { (diffs as f64) * 100.0 / (a.len() as f64) };
    json!({
        "same_size":true,
        "len_a":a.len(),
        "len_b":b.len(),
        "diff_bytes":diffs,
        "same_bytes":a.len()-diffs,
        "diff_percent": (pct * 1000.0).round() / 1000.0,
        "identical":diffs==0,
        "first_diffs":first,
        "diff_ranges": ranges,
        "diff_range_count": ranges.len(),
        "message": if diffs==0 {"Images are identical."} else {"Images differ."}
    })
}

fn analyze_log() -> Result<serde_json::Value, String> {
    let samples = logging::get_samples(Some(50_000));
    if samples.is_empty() { return Err("No log samples. Start a session and capture data first.".into()); }
    let mut rpm_sum=0.0; let mut n_rpm=0.0; let mut map_sum=0.0; let mut n_map=0.0;
    let mut grid = vec![vec![0u32;16];16];
    let mut stft_sum = vec![vec![0.0f64;16];16];
    let mut stft_n = vec![vec![0u32;16];16];
    for s in &samples {
        if let Some(v)=s.values.get("rpm") { rpm_sum += v; n_rpm += 1.0; }
        if let Some(v)=s.values.get("map") { map_sum += v; n_map += 1.0; }
        let rpm = s.values.get("rpm").copied().unwrap_or(0.0);
        let mapv = s.values.get("map").copied().unwrap_or(0.0);
        let r = ((rpm/500.0).floor() as i32).clamp(0,15) as usize;
        let c = ((mapv/16.0).floor() as i32).clamp(0,15) as usize;
        grid[r][c] = grid[r][c].saturating_add(1);
        if let Some(stft) = s.values.get("stft") {
            stft_sum[r][c] += *stft;
            stft_n[r][c] = stft_n[r][c].saturating_add(1);
        }
    }
    let mut stft_avg = vec![vec![None;16];16];
    for r in 0..16 {
        for c in 0..16 {
            if stft_n[r][c] > 0 {
                stft_avg[r][c] = Some(((stft_sum[r][c] / stft_n[r][c] as f64) * 10.0).round() / 10.0);
            }
        }
    }
    let rpm_avg = if n_rpm>0.0 { rpm_sum/n_rpm } else { 0.0 };
    let map_avg = if n_map>0.0 { map_sum/n_map } else { 0.0 };
    let mut hottest = (0usize,0usize,0u32);
    for r in 0..16 { for c in 0..16 { if grid[r][c] > hottest.2 { hottest = (r,c,grid[r][c]); } } }
    Ok(json!({
        "sample_count": samples.len(),
        "rpm_avg": (rpm_avg*10.0).round()/10.0,
        "map_avg_kpa": (map_avg*10.0).round()/10.0,
        "suggested_ve_cell": {"row_rpm": ((rpm_avg/500.0).floor() as i32).clamp(0,15), "col_map": ((map_avg/16.0).floor() as i32).clamp(0,15)},
        "hottest_cell": {"row_rpm":hottest.0,"col_map":hottest.1,"hits":hottest.2},
        "occupancy_16x16": grid,
        "stft_avg_16x16": stft_avg,
        "advice": format!("Hottest cell r{} c{} ({} hits). Mean {:.0} RPM / {:.0} kPa. Hint only — not auto-write.", hottest.0, hottest.1, hottest.2, rpm_avg, map_avg)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn identify_unknown_size() { let v = identify_bin(&[0u8;64]); assert_eq!(v["bin_size_bytes"], 64); assert!(v["sha256"].as_str().unwrap().len() == 64); }
    #[test] fn identify_p01_size() {
        let v = identify_bin(&vec![0u8;524288]);
        assert_eq!(v["family_by_size"], "P01_0411");
        assert!(v["family"].is_null(), "512KB collides with Honda — family stays unset without an OS string");
        assert_eq!(v["size_collision"], true);
        assert_eq!(v["correction_safe"], false);
    }
    #[test] fn identify_honda_os() {
        let mut img = vec![0u8; 524288];
        img[0x40..0x48].copy_from_slice(b"37820-PR");
        let v = identify_bin(&img);
        assert_eq!(v["family"], "HONDA_KEIHIN");
        assert_eq!(v["honda_os"], true);
        assert_eq!(v["correction_safe"], false);
        assert!(resolved_family(&img).unwrap_err().contains("Honda"));
    }
    #[test] fn resolved_family_refuses_512k_collision() {
        let img = vec![0u8; 524288];
        let err = resolved_family(&img).unwrap_err();
        assert!(err.contains("collides") || err.contains("OS"), "{}", err);
    }
    #[test] fn import_workspace_json() {
        let raw = import_workspace_cmd(r#"{"tool":"TuneItVerse","version":"3.10.0","identify":{"family":"P01_0411"}}"#.into()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["accepted"], true);
        assert_eq!(v["has_identify"], true);
    }
    #[test] fn identify_os_string() {
        let mut img = vec![0u8; 128];
        img[10..18].copy_from_slice(b"12225074");
        let v = identify_bin(&img);
        assert_eq!(v["family_by_os"], "P01_0411");
        assert_eq!(v["family"], "P01_0411");
    }
    #[test] fn poke_bytes() {
        let img = vec![0u8; 8];
        let out = patch_bin_bytes_cmd(img, 2, vec![0xAA, 0xBB]).unwrap();
        assert_eq!(out[2], 0xAA);
        assert_eq!(out[3], 0xBB);
    }
    #[test] fn identify_me7_size() { let v = identify_bin(&vec![0u8;1048576]); assert_eq!(v["family_by_size"], "ME7_COMMON"); }
    #[test] fn compare_same() { let a=vec![1u8,2,3,4]; assert_eq!(compare_bins(&a,&a)["identical"], true); }
    #[test] fn compare_range() {
        let a=vec![0u8;8]; let mut b=a.clone(); b[2]=1; b[3]=1;
        let v = compare_bins(&a,&b);
        assert_eq!(v["diff_bytes"], 2);
        assert_eq!(v["diff_range_count"], 1);
    }
}
