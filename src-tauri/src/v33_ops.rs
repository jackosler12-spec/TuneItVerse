//! v3.3.0 ops that sit on top of existing public logging/security APIs.
//! Keeps lib.rs command names stable without requiring private LOG access.

use serde_json::json;
use std::collections::HashMap;
use crate::logging;
use crate::security;

pub fn import_csv_cmd(csv: &str) -> Result<String, String> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or_else(|| "CSV is empty".to_string())?;
    let cols: Vec<String> = header.split(',').map(|s| s.trim().trim_matches('"').to_ascii_lowercase()).collect();
    if cols.is_empty() {
        return Err("CSV header is empty".into());
    }
    let ts_idx = cols.iter().position(|c| c == "timestamp_ms" || c == "time" || c == "t");
    let _ = logging::stop_session();
    let _ = logging::clear_samples();
    logging::ensure_initialized();
    logging::start_session(Some(10.0), Some("imported".into()))?;
    let mut count = 0usize;
    for line in lines {
        let cells: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        let mut values = HashMap::new();
        for (i, name) in cols.iter().enumerate() {
            if ts_idx == Some(i) || name.is_empty() {
                continue;
            }
            if let Some(cell) = cells.get(i) {
                if let Ok(v) = cell.parse::<f64>() {
                    values.insert(name.clone(), v);
                }
            }
        }
        if values.is_empty() {
            continue;
        }
        let ids: Vec<String> = values.keys().cloned().collect();
        let _ = logging::set_channels(ids);
        logging::capture_sample(Some(values))?;
        count += 1;
        if count >= 50_000 {
            break;
        }
    }
    let st = logging::stop_session()?;
    Ok(serde_json::to_string(&st).unwrap_or_else(|_| json!({"sample_count": count}).to_string()))
}

fn parse_seed_hex(seed_hex: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = seed_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.is_empty() {
        return Err("Seed hex is empty".into());
    }
    let padded = if cleaned.len() % 2 == 1 { format!("0{}", cleaned) } else { cleaned };
    let bytes = padded.as_bytes();
    let mut seed = Vec::with_capacity(padded.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        seed.push((hi << 4) | lo);
        i += 2;
    }
    Ok(seed)
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("Invalid hex digit {}", b as char)),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

pub fn compute_seed_key_cmd(family: &str, seed_hex: &str, level: Option<&str>) -> Result<String, String> {
    let seed = parse_seed_hex(seed_hex)?;
    let fam = family.to_ascii_uppercase();
    let lvl = level.unwrap_or("").to_ascii_lowercase();
    let (algo, key) = if fam.contains("P01") || fam.contains("P59") || fam.contains("GM") {
        if seed.len() < 2 {
            return Err("GM P01/P59 seed must be at least 2 bytes".into());
        }
        let (kh, kl) = if lvl.contains('2') || lvl.contains("flash") || lvl.contains("prog") {
            security::p01_key_l2(seed[0], seed[1])
        } else {
            security::p01_key_l1(seed[0], seed[1])
        };
        ("gm_p01_lfsr16", vec![kh, kl])
    } else {
        ("bosch_family_dispatcher", security::bosch_key_from_seed(&seed, family))
    };
    Ok(json!({
        "success": true,
        "family": family,
        "level": level.unwrap_or("auto"),
        "algo": algo,
        "seed_hex": hex_encode(&seed),
        "key_hex": hex_encode(&key),
        "seed_len": seed.len(),
        "key_len": key.len(),
        "notes": "Offline calculator only. Live unlock still requires a connected adapter."
    }).to_string())
}
