//! Honda / P01 size-collision helpers and report-only checksum window scan.
use crate::checksum_sizes::is_p01_size;

pub fn looks_like_honda(data: &[u8]) -> bool {
    let mut cur = String::new();
    for &b in data {
        if (0x20..=0x7E).contains(&b) {
            cur.push(b as char);
            if cur.len() > 32 {
                cur = cur[cur.len() - 32..].to_string();
            }
            let up = cur.to_ascii_uppercase();
            if up.contains("37820") || up.contains("KEIHIN") || up.contains("K20A") || up.contains("K24A") {
                return true;
            }
        } else {
            cur.clear();
        }
    }
    false
}

pub fn looks_like_gm_p01(data: &[u8]) -> bool {
    let mut cur = String::new();
    for &b in data {
        if (0x20..=0x7E).contains(&b) {
            cur.push(b as char);
            if cur.len() > 32 {
                cur = cur[cur.len() - 32..].to_string();
            }
            let up = cur.to_ascii_uppercase();
            if up.contains("12225074") || up.contains("12200411") || up.contains("P01") {
                return true;
            }
        } else {
            cur.clear();
        }
    }
    false
}

pub fn honda_blocks_p01_corrector(data: &[u8]) -> bool {
    is_p01_size(data.len()) && looks_like_honda(data) && !looks_like_gm_p01(data)
}

pub fn scan_checksum_candidates(data: &[u8]) -> serde_json::Value {
    let mut hits = Vec::new();
    if data.len() < 4 {
        return serde_json::json!({"candidates": [], "note": "image too small"});
    }
    let step = 0x10000.min(data.len());
    let mut off = 0usize;
    while off + 4 <= data.len() {
        let end = (off + step).min(data.len()) - 1;
        let mut sum: u16 = 0;
        let mut i = off;
        while i + 1 <= end {
            sum = sum.wrapping_add(((data[i] as u16) << 8) | data[i + 1] as u16);
            i += 2;
        }
        if sum == 0 {
            hits.push(serde_json::json!({
                "start": format!("0x{:06X}", off),
                "end": format!("0x{:06X}", end),
                "sum16": "0x0000",
                "hint": "already-zero additive window"
            }));
        }
        if hits.len() >= 32 {
            break;
        }
        if off + step >= data.len() {
            break;
        }
        off += step;
    }
    serde_json::json!({
        "bytes": data.len(),
        "window": step,
        "candidates": hits,
        "honda_os": looks_like_honda(data),
        "gm_p01_os": looks_like_gm_p01(data),
        "honda_blocks_p01": honda_blocks_p01_corrector(data),
        "note": "Report-only. A zero-sum window is not a corrector. Do not invent CS bytes."
    })
}

#[tauri::command]
pub fn scan_checksum_candidates_cmd(data: Vec<u8>) -> Result<String, String> {
    Ok(scan_checksum_candidates(&data).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn honda_string_detected() {
        let mut img = vec![0u8; 64];
        img[10..18].copy_from_slice(b"37820-PR");
        assert!(looks_like_honda(&img));
        assert!(!looks_like_gm_p01(&img));
    }
    #[test]
    fn tiny_scan_empty() {
        let v = scan_checksum_candidates(&[1, 2, 3]);
        assert_eq!(v["candidates"].as_array().unwrap().len(), 0);
    }
}
