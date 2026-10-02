//! v3.36 session tools.
//! Family fingerprint scores, log channel stats, tune project files, BIN search.
//! None of these enable a write path.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

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
        } else if cur.len() >= min_len {
            out.push(std::mem::take(&mut cur));
            if out.len() >= limit {
                break;
            }
        } else {
            cur.clear();
        }
    }
    if cur.len() >= min_len && out.len() < limit {
        out.push(cur);
    }
    out
}

fn tokens(raw: &str) -> Vec<String> {
    raw.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 5 && t.len() <= 16 && t.chars().any(|c| c.is_ascii_digit()))
        .map(|t| t.to_ascii_uppercase())
        .collect()
}

/// Rank catalog families. Size alone never wins a collision. Write is not decided here.
pub fn score_fingerprint(data: &[u8]) -> Value {
    let strings = printable_strings(data, 5, 80);
    let hay: Vec<String> = strings.iter().map(|s| s.to_ascii_uppercase()).collect();
    let honda = crate::cs_guard::looks_like_honda(data);
    let p01 = crate::cs_guard::looks_like_gm_p01(data);
    let p59 = crate::cs_guard::looks_like_gm_p59(data);
    let mut ranked = Vec::new();
    for fam in crate::ecu_database::list_supported_ecu_families() {
        let Some(entry) = crate::ecu_database::get_ecu_by_family(&fam) else { continue };
        let mut score = 0i32;
        let mut reasons = Vec::new();
        if entry.bin_size_bytes as usize == data.len() {
            score += 2;
            reasons.push("size match".to_string());
        }
        let mut matched = Vec::new();
        for raw in &entry.part_numbers_or_os_ids {
            for tok in tokens(raw) {
                if hay.iter().any(|s| s.contains(&tok)) && !matched.contains(&tok) {
                    matched.push(tok);
                }
            }
        }
        if !matched.is_empty() {
            score += 5 * matched.len() as i32;
            reasons.push(format!("os/part {}", matched.join(",")));
        }
        if fam.eq_ignore_ascii_case("HONDA_KEIHIN") && honda {
            score += 8;
            reasons.push("honda OS string".into());
        }
        if fam.eq_ignore_ascii_case("P01_0411") && p01 && !honda && !p59 {
            score += 8;
            reasons.push("gm p01 OS string".into());
        }
        if fam.eq_ignore_ascii_case("GM_P59") && p59 && !p01 {
            score += 8;
            reasons.push("gm p59 OS string".into());
        }
        if score == 0 {
            continue;
        }
        ranked.push(json!({
            "family": entry.ecu_family,
            "display_name": entry.display_name,
            "score": score,
            "reasons": reasons,
            "write_path_live": crate::ecu_database::write_path_live(&entry.ecu_family),
        }));
    }
    ranked.sort_by(|a, b| b["score"].as_i64().unwrap_or(0).cmp(&a["score"].as_i64().unwrap_or(0)));
    let top = ranked.first().and_then(|r| r.get("family")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let collision = ranked.iter().filter(|r| r["score"].as_i64().unwrap_or(0) == ranked.first().and_then(|x| x["score"].as_i64()).unwrap_or(0)).count() > 1;
    json!({
        "bin_size_bytes": data.len(),
        "sha256": sha256_hex(data),
        "top_family": top,
        "collision": collision,
        "ranked": ranked,
        "printable_strings": strings.into_iter().take(16).collect::<Vec<_>>(),
        "write_allowed": false,
        "notes": "Fingerprint is a rank, not a write grant. Live write stays P01_0411 and EDC16C41 only, and only after identify says correction_safe."
    })
}

pub fn channel_stats(samples: &[crate::logging::LogSample]) -> Result<Value, String> {
    if samples.is_empty() {
        return Err("No log samples. Import a CSV or capture a session first.".into());
    }
    let mut acc: std::collections::BTreeMap<String, Vec<f64>> = std::collections::BTreeMap::new();
    for s in &samples {
        for (k, v) in &s.values {
            acc.entry(k.clone()).or_default().push(*v);
        }
    }
    if acc.is_empty() {
        return Err("Samples exist but every channel is empty. Nothing was invented.".into());
    }
    let mut channels = Vec::new();
    let mut flags = Vec::new();
    for (id, vals) in &acc {
        let n = vals.len() as f64;
        let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = vals.iter().sum::<f64>() / n;
        let var = vals.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / n;
        let std = var.sqrt();
        channels.push(json!({
            "id": id,
            "count": vals.len(),
            "min": (min * 1000.0).round() / 1000.0,
            "max": (max * 1000.0).round() / 1000.0,
            "avg": (avg * 1000.0).round() / 1000.0,
            "stddev": (std * 1000.0).round() / 1000.0,
        }));
        if id == "stft" && vals.iter().any(|v| v.abs() > 15.0) {
            flags.push("STFT exceeded ±15% on at least one sample. Hint only — not an auto VE write.".to_string());
        }
        if id == "ect" && max > 105.0 {
            flags.push(format!("ECT peaked at {max:.1} C. Check cooling before any load pull."));
        }
        if id == "batt" && min < 12.0 {
            flags.push(format!("Battery dipped to {min:.2} V in this log. Do not flash on this supply."));
        }
    }
    Ok(json!({
        "sample_count": samples.len(),
        "channel_count": channels.len(),
        "channels": channels,
        "flags": flags,
        "advice": "Stats come only from samples that were captured or imported. Empty channels stay absent."
    }))
}

pub fn analyze_log_channels(csv: Option<&str>) -> Result<Value, String> {
    if let Some(raw) = csv {
        if !raw.trim().is_empty() {
            crate::logging::import_csv(raw)?;
        }
    }
    channel_stats(&crate::logging::get_samples(Some(50_000)))
}

pub fn build_tune_project(data: &[u8], notes: &str) -> Value {
    let ident = crate::v29_tools::identify_bin(data);
    let fp = score_fingerprint(data);
    json!({
        "tool": "TuneItVerse",
        "kind": "tune_project",
        "version": crate::APP_VERSION,
        "notes": notes,
        "sha256": sha256_hex(data),
        "bin_size_bytes": data.len(),
        "identify": ident,
        "fingerprint": fp,
        "write_allowed": ident.get("write_allowed").and_then(|v| v.as_bool()).unwrap_or(false),
        "policy": "Project JSON cannot enable write. Reload the BIN and run identify before any flash."
    })
}

pub fn load_tune_project(text: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("tune project JSON: {e}"))?;
    if v.get("kind").and_then(|k| k.as_str()) != Some("tune_project") && v.get("tool").and_then(|k| k.as_str()) != Some("TuneItVerse") {
        return Err("not a TuneItVerse project file".into());
    }
    Ok(json!({
        "accepted": true,
        "version": v.get("version").and_then(|x| x.as_str()).unwrap_or("unknown"),
        "notes": v.get("notes").and_then(|x| x.as_str()).unwrap_or(""),
        "sha256": v.get("sha256"),
        "identify": v.get("identify"),
        "fingerprint": v.get("fingerprint"),
        "write_allowed": false,
        "note": "Loaded metadata only. write_allowed from the file is ignored. BIN bytes are not restored."
    }))
}

pub fn search_bin(data: &[u8], query: &str) -> Result<Value, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("search query is empty".into());
    }
    let hex_only = q.chars().all(|c| c.is_ascii_hexdigit() || c.is_ascii_whitespace());
    let needle = if hex_only && q.chars().filter(|c| c.is_ascii_hexdigit()).count() >= 4 && q.chars().filter(|c| c.is_ascii_hexdigit()).count() % 2 == 0 {
        let cleaned: String = q.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        hex::decode_simple(&cleaned)?
    } else {
        q.as_bytes().to_vec()
    };
    if needle.is_empty() || needle.len() > 64 {
        return Err("needle must be 1..64 bytes".into());
    }
    let mut hits = Vec::new();
    if data.len() >= needle.len() {
        for i in 0..=data.len() - needle.len() {
            if &data[i..i + needle.len()] == needle.as_slice() {
                hits.push(format!("0x{i:06X}"));
                if hits.len() >= 32 {
                    break;
                }
            }
        }
    }
    Ok(json!({
        "query": q,
        "needle_len": needle.len(),
        "hit_count": hits.len(),
        "truncated": hits.len() == 32,
        "offsets": hits
    }))
}

mod hex {
    pub fn decode_simple(s: &str) -> Result<Vec<u8>, String> {
        if s.len() % 2 != 0 {
            return Err("hex needle must be even length".into());
        }
        let mut out = Vec::with_capacity(s.len() / 2);
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let hi = hex_val(b[i])?;
            let lo = hex_val(b[i + 1])?;
            out.push((hi << 4) | lo);
            i += 2;
        }
        Ok(out)
    }
    fn hex_val(c: u8) -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err("invalid hex".into()),
        }
    }
}

#[tauri::command]
pub fn score_fingerprint_cmd(data: Vec<u8>) -> Result<String, String> {
    Ok(score_fingerprint(&data).to_string())
}

#[tauri::command]
pub fn analyze_log_channels_cmd(csv: Option<String>) -> Result<String, String> {
    analyze_log_channels(csv.as_deref()).map(|v| v.to_string())
}

#[tauri::command]
pub fn build_tune_project_cmd(data: Vec<u8>, notes: Option<String>) -> Result<String, String> {
    Ok(build_tune_project(&data, notes.as_deref().unwrap_or("")).to_string())
}

#[tauri::command]
pub fn load_tune_project_cmd(json_text: String) -> Result<String, String> {
    load_tune_project(&json_text).map(|v| v.to_string())
}

#[tauri::command]
pub fn search_bin_cmd(data: Vec<u8>, query: String) -> Result<String, String> {
    search_bin(&data, &query).map(|v| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_512k_does_not_grant_write() {
        let img = vec![0u8; 524288];
        let v = score_fingerprint(&img);
        assert_eq!(v["write_allowed"], false);
        assert!(v["ranked"].as_array().unwrap().iter().all(|r| r["write_path_live"] != true || r["score"].as_i64().unwrap_or(0) < 5));
    }

    #[test]
    fn honda_string_outranks_size_collision() {
        let mut img = vec![0u8; 524288];
        img[0x40..0x48].copy_from_slice(b"37820-PR");
        let v = score_fingerprint(&img);
        assert_eq!(v["top_family"], "HONDA_KEIHIN");
        assert_eq!(v["write_allowed"], false);
    }

    #[test]
    fn project_ignores_write_flag_on_load() {
        let raw = r#"{"tool":"TuneItVerse","kind":"tune_project","version":"9.9.9","write_allowed":true,"notes":"bench"}"#;
        let v = load_tune_project(raw).unwrap();
        assert_eq!(v["write_allowed"], false);
        assert_eq!(v["accepted"], true);
    }

    #[test]
    fn search_finds_ascii_and_hex() {
        let mut img = vec![0u8; 64];
        img[10..18].copy_from_slice(b"12225074");
        let hits = search_bin(&img, "12225074").unwrap();
        assert_eq!(hits["offsets"][0], "0x00000A");
        img[20] = 0xDE;
        img[21] = 0xAD;
        let hex_hits = search_bin(&img, "DEAD").unwrap();
        assert_eq!(hex_hits["offsets"][0], "0x000014");
    }

    #[test]
    fn log_stats_use_supplied_rows_only() {
        use crate::logging::LogSample;
        use std::collections::HashMap;
        let mut a = HashMap::new();
        a.insert("rpm".into(), 2000.0);
        a.insert("map".into(), 40.0);
        let mut b = HashMap::new();
        b.insert("rpm".into(), 4000.0);
        b.insert("map".into(), 80.0);
        let samples = vec![
            LogSample { timestamp_ms: 1, values: a },
            LogSample { timestamp_ms: 2, values: b },
        ];
        let v = channel_stats(&samples).unwrap();
        assert_eq!(v["sample_count"], 2);
        let rpm = v["channels"].as_array().unwrap().iter().find(|c| c["id"] == "rpm").unwrap();
        assert_eq!(rpm["min"], 2000.0);
        assert_eq!(rpm["max"], 4000.0);
        assert!(v["flags"].as_array().unwrap().is_empty());
    }
}
