//! Log studio and BIN diff report.
//! Preview only. Nothing here sets write_allowed or patches a calibration.

use serde_json::{json, Value};
use std::collections::HashMap;

use crate::logging::{self, LogSample};

const RPM_ALIASES: &[&str] = &["rpm", "engine_rpm", "engine speed", "revs"];
const LOAD_ALIASES: &[&str] = &["map", "load", "maf", "imap", "boost"];
const TRIM_ALIASES: &[&str] = &[
    "stft",
    "ltft",
    "stft_b1",
    "short_term_fuel_trim",
    "long_term_fuel_trim",
    "fuel_trim",
];

fn pick(values: &HashMap<String, f64>, names: &[&str]) -> Option<f64> {
    for name in names {
        if let Some(v) = values.get(*name) {
            return Some(*v);
        }
        let want = name.to_ascii_lowercase();
        if let Some((_, v)) = values.iter().find(|(k, _)| k.to_ascii_lowercase() == want) {
            return Some(*v);
        }
    }
    None
}

pub fn summarize_samples(samples: &[LogSample]) -> Value {
    let mut channels: HashMap<String, (f64, f64, f64, u32)> = HashMap::new();
    let mut t0 = None;
    let mut t1 = None;
    for sample in samples {
        t0 = Some(t0.map(|t: u64| t.min(sample.timestamp_ms)).unwrap_or(sample.timestamp_ms));
        t1 = Some(t1.map(|t: u64| t.max(sample.timestamp_ms)).unwrap_or(sample.timestamp_ms));
        for (id, value) in &sample.values {
            if !value.is_finite() {
                continue;
            }
            let entry = channels.entry(id.clone()).or_insert((f64::MAX, f64::MIN, 0.0, 0));
            entry.0 = entry.0.min(*value);
            entry.1 = entry.1.max(*value);
            entry.2 += *value;
            entry.3 += 1;
        }
    }
    let mut stats: Vec<Value> = channels
        .into_iter()
        .map(|(id, (min, max, sum, n))| {
            json!({
                "id": id,
                "min": round1(min),
                "max": round1(max),
                "mean": round1(if n == 0 { 0.0 } else { sum / n as f64 }),
                "samples": n
            })
        })
        .collect();
    stats.sort_by(|a, b| a["id"].as_str().unwrap_or("").cmp(b["id"].as_str().unwrap_or("")));
    let duration_ms = t0.zip(t1).map(|(a, b)| b.saturating_sub(a)).unwrap_or(0);
    json!({
        "sample_count": samples.len(),
        "duration_ms": duration_ms,
        "channels": stats,
        "write_allowed": false,
        "note": "Session summary is offline. It does not talk to an ECU and does not enable write."
    })
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// STFT/LTFT occupancy into a 16x16 RPM x load preview.
/// Suggested multiplier is clamped. Caller must not write it back silently.
pub fn trim_preview(samples: &[LogSample]) -> Value {
    let mut hits = vec![vec![0u32; 16]; 16];
    let mut trim_sum = vec![vec![0.0f64; 16]; 16];
    let mut trim_n = vec![vec![0u32; 16]; 16];
    let mut used_trim = false;
    for sample in samples {
        let Some(rpm) = pick(&sample.values, RPM_ALIASES) else { continue };
        let Some(load) = pick(&sample.values, LOAD_ALIASES) else { continue };
        let row = ((rpm / 500.0).floor() as i32).clamp(0, 15) as usize;
        let col = ((load / 16.0).floor() as i32).clamp(0, 15) as usize;
        hits[row][col] = hits[row][col].saturating_add(1);
        if let Some(trim) = pick(&sample.values, TRIM_ALIASES) {
            used_trim = true;
            trim_sum[row][col] += trim;
            trim_n[row][col] = trim_n[row][col].saturating_add(1);
        }
    }
    let mut multiplier = vec![vec![1.0f64; 16]; 16];
    let mut touched = 0u32;
    for row in 0..16 {
        for col in 0..16 {
            if trim_n[row][col] == 0 {
                continue;
            }
            let avg = trim_sum[row][col] / trim_n[row][col] as f64;
            let factor = (1.0 + avg / 100.0).clamp(0.85, 1.15);
            multiplier[row][col] = (factor * 1000.0).round() / 1000.0;
            touched += 1;
        }
    }
    json!({
        "rows": 16,
        "cols": 16,
        "rpm_step": 500,
        "load_step": 16,
        "hits": hits,
        "multiplier": multiplier,
        "cells_with_trim": touched,
        "trim_column_found": used_trim,
        "write_allowed": false,
        "advice": if used_trim {
            "Preview only. Multiplier is 1 + trim%/100, clamped 0.85–1.15. Apply it yourself in the map editor. This does not patch the BIN."
        } else {
            "No STFT/LTFT column found. Occupancy is still filled from RPM and load/MAP. No multiplier change."
        }
    })
}

pub fn diff_report(a: &[u8], b: &[u8]) -> String {
    if a.len() != b.len() {
        return format!(
            "# BIN diff\n\nImages differ in length ({} vs {}). No byte overlay.\n\nThis report does not enable write.\n",
            a.len(),
            b.len()
        );
    }
    let mut diffs = 0usize;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            diffs += 1;
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            ranges.push((s, i - 1));
        }
    }
    if let Some(s) = start {
        ranges.push((s, a.len().saturating_sub(1)));
    }
    let pct = if a.is_empty() { 0.0 } else { diffs as f64 * 100.0 / a.len() as f64 };
    let mut out = format!(
        "# BIN diff\n\n- Length: {}\n- Changed bytes: {} ({:.4}%)\n- Ranges: {}\n- write_allowed: false\n\n",
        a.len(),
        diffs,
        pct,
        ranges.len()
    );
    if ranges.is_empty() {
        out.push_str("Images are identical.\n");
        return out;
    }
    out.push_str("| start | end | length |\n|---|---|---|\n");
    for (s, e) in ranges.iter().take(80) {
        out.push_str(&format!("| 0x{s:06X} | 0x{e:06X} | {} |\n", e - s + 1));
    }
    if ranges.len() > 80 {
        out.push_str(&format!("\n{} further ranges omitted.\n", ranges.len() - 80));
    }
    out.push_str("\nPersonal dumps only. Diff does not patch either image.\n");
    out
}

pub fn self_check_ok() -> bool {
    let summary = summarize_samples(&[]);
    let preview = trim_preview(&[]);
    let report = diff_report(&[1, 2, 3, 4], &[1, 9, 3, 4]);
    summary["write_allowed"] == false
        && preview["write_allowed"] == false
        && report.contains("0x000001")
        && report.contains("write_allowed: false")
}

#[tauri::command]
pub fn log_session_summary() -> Result<String, String> {
    let samples = logging::get_samples(Some(50_000));
    if samples.is_empty() {
        return Err("No log samples. Import a CSV or capture a session first.".into());
    }
    Ok(summarize_samples(&samples).to_string())
}

#[tauri::command]
pub fn log_replay_frame(index: Option<usize>) -> Result<String, String> {
    let samples = logging::get_samples(Some(50_000));
    if samples.is_empty() {
        return Err("No log samples to replay.".into());
    }
    let index = index.unwrap_or(0).min(samples.len() - 1);
    let sample = &samples[index];
    Ok(json!({
        "index": index,
        "count": samples.len(),
        "timestamp_ms": sample.timestamp_ms,
        "values": sample.values,
        "write_allowed": false
    })
    .to_string())
}

#[tauri::command]
pub fn log_trim_suggestion() -> Result<String, String> {
    let samples = logging::get_samples(Some(50_000));
    if samples.is_empty() {
        return Err("No log samples. Import a CSV with rpm, map/load, and stft/ltft.".into());
    }
    Ok(trim_preview(&samples).to_string())
}

#[tauri::command]
pub fn bin_diff_report(a: Vec<u8>, b: Vec<u8>) -> Result<String, String> {
    Ok(diff_report(&a, &b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ms: u64, rpm: f64, map: f64, stft: f64) -> LogSample {
        let mut values = HashMap::new();
        values.insert("rpm".into(), rpm);
        values.insert("map".into(), map);
        values.insert("stft".into(), stft);
        LogSample { timestamp_ms: ms, values }
    }

    #[test]
    fn summary_tracks_min_max_and_blocks_write() {
        let rows = vec![sample(0, 1000.0, 40.0, 5.0), sample(1000, 2000.0, 80.0, -5.0)];
        let v = summarize_samples(&rows);
        assert_eq!(v["sample_count"], 2);
        assert_eq!(v["duration_ms"], 1000);
        assert_eq!(v["write_allowed"], false);
        let rpm = v["channels"].as_array().unwrap().iter().find(|c| c["id"] == "rpm").unwrap();
        assert_eq!(rpm["min"], 1000.0);
        assert_eq!(rpm["max"], 2000.0);
    }

    #[test]
    fn trim_preview_clamps_and_does_not_write() {
        let rows = vec![sample(0, 1500.0, 40.0, 50.0)];
        let v = trim_preview(&rows);
        assert_eq!(v["write_allowed"], false);
        assert_eq!(v["cells_with_trim"], 1);
        let factor = v["multiplier"][3][2].as_f64().unwrap();
        assert!((factor - 1.15).abs() < 0.001);
    }

    #[test]
    fn diff_report_lists_changed_range() {
        let text = diff_report(&[0, 1, 2, 3], &[0, 9, 8, 3]);
        assert!(text.contains("0x000001"));
        assert!(text.contains("write_allowed: false"));
    }
}
