//! Log studio and BIN diff report.
//! Preview only. Nothing here sets write_allowed or patches a calibration.

use serde::Deserialize;
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

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ReportTable {
    #[serde(default)]
    name: String,
    #[serde(default)]
    addr: String,
    #[serde(default)]
    rows: usize,
    #[serde(default)]
    cols: usize,
    #[serde(default)]
    data_type: String,
    #[serde(default)]
    stride_bytes: usize,
}

fn parse_addr(s: &str) -> Option<usize> {
    let t = s.trim();
    if t.is_empty() { return None; }
    let hex = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X"));
    if let Some(h) = hex {
        return usize::from_str_radix(h, 16).ok();
    }
    if t.chars().all(|c| c.is_ascii_hexdigit()) && t.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        return usize::from_str_radix(t, 16).ok();
    }
    t.parse::<usize>().ok()
}

fn table_span(t: &ReportTable) -> Option<(usize, usize, String)> {
    let addr = parse_addr(&t.addr)?;
    let width = crate::xdf::cell_width(&t.data_type);
    if width == 0 || t.rows == 0 || t.cols == 0 { return None; }
    let cells = t.rows.saturating_mul(t.cols);
    if cells == 0 { return None; }
    let len = if t.stride_bytes > width {
        t.stride_bytes.saturating_mul(cells - 1).saturating_add(width)
    } else {
        width.saturating_mul(cells)
    };
    if len == 0 { return None; }
    let name = if t.name.trim().is_empty() { format!("0x{addr:X}") } else { t.name.trim().to_string() };
    Some((addr, len, name))
}

fn hex_preview(data: &[u8], start: usize, end: usize) -> String {
    let last = end.min(data.len().saturating_sub(1));
    if start > last || start >= data.len() { return String::new(); }
    let n = (last - start + 1).min(8);
    let mut s = data[start..start + n].iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
    if last - start + 1 > 8 { s.push_str(" ..."); }
    s
}

/// Shop sheet: changed ranges, overlapping table names, and a short old/new hex preview.
/// Names come only from tables the caller already parsed. This does not write a BIN.
pub fn change_report(a: &[u8], b: &[u8], tables: &[ReportTable]) -> String {
    if a.len() != b.len() {
        return format!(
            "# Tune change report\n\nImages differ in length ({} vs {}). No byte overlay.\n\nwrite_allowed: false\nThis report does not enable write.\n",
            a.len(), b.len()
        );
    }
    let spans: Vec<(usize, usize, String)> = tables.iter().filter_map(table_span).collect();
    let mut diffs = 0usize;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            diffs += 1;
            if start.is_none() { start = Some(i); }
        } else if let Some(s) = start.take() {
            ranges.push((s, i - 1));
        }
    }
    if let Some(s) = start { ranges.push((s, a.len().saturating_sub(1))); }
    let pct = if a.is_empty() { 0.0 } else { diffs as f64 * 100.0 / a.len() as f64 };
    let mut out = format!(
        "# Tune change report\n\n- Length: {}\n- Changed bytes: {} ({:.4}%)\n- Ranges: {}\n- Named from {} loaded table(s)\n- write_allowed: false\n\n",
        a.len(), diffs, pct, ranges.len(), spans.len()
    );
    if ranges.is_empty() {
        out.push_str("Images are identical.\n\nThis report does not patch either image.\n");
        return out;
    }
    out.push_str("| start | end | length | old | new | tables |\n|---|---|---:|---|---|---|\n");
    for (s, e) in ranges.iter().take(80) {
        let names = spans.iter().filter(|(addr, len, _)| {
            let end = addr.saturating_add(*len);
            *addr <= *e && end > *s
        }).map(|(_, _, n)| n.as_str()).collect::<Vec<_>>().join(", ");
        let label = if names.is_empty() { "—".to_string() } else { names };
        out.push_str(&format!(
            "| 0x{s:06X} | 0x{e:06X} | {} | {} | {} | {label} |\n",
            e - s + 1,
            hex_preview(a, *s, *e),
            hex_preview(b, *s, *e)
        ));
    }
    if ranges.len() > 80 {
        out.push_str(&format!("\n{} further ranges omitted.\n", ranges.len() - 80));
    }
    out.push_str("\nPersonal dumps only. This report does not patch either image.\n");
    out
}

#[tauri::command]
pub fn change_report_cmd(a: Vec<u8>, b: Vec<u8>, tables: Option<Vec<ReportTable>>) -> Result<String, String> {
    Ok(change_report(&a, &b, tables.unwrap_or_default().as_slice()))
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

/// Cite logged rows. Does not emit a calibration value or a map edit.
pub fn explain_log(samples: &[LogSample]) -> String {
    let mut channels: HashMap<String, (f64, f64, f64, u32)> = HashMap::new();
    for sample in samples {
        for (name, value) in &sample.values {
            let entry = channels.entry(name.clone()).or_insert((f64::MAX, f64::MIN, 0.0, 0));
            entry.0 = entry.0.min(*value);
            entry.1 = entry.1.max(*value);
            entry.2 += *value;
            entry.3 += 1;
        }
    }
    let mut out = String::from(
        "# Log explainer\n\nThis report cites logged rows and does not emit a calibration value.\nwrite_allowed: false\n\n",
    );
    if samples.iter().any(|sample| sample.demo) {
        out.push_str("One or more rows are demo samples, not a vehicle log.\n\n");
    }
    out.push_str(&format!("Rows cited: {}\n\n", samples.len()));
    let shown = samples.len().min(8);
    for (index, sample) in samples.iter().take(shown).enumerate() {
        let mut parts: Vec<String> = sample
            .values
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        parts.sort();
        out.push_str(&format!(
            "- Row {index} at {} ms: {}\n",
            sample.timestamp_ms,
            parts.join(", ")
        ));
    }
    if samples.len() > shown {
        out.push_str(&format!(
            "- {} further rows omitted from the row list.\n",
            samples.len() - shown
        ));
    }
    out.push('\n');
    let mut names: Vec<&String> = channels.keys().collect();
    names.sort();
    for name in names {
        let (min, max, sum, count) = channels[name];
        let mean = if count == 0 { 0.0 } else { sum / count as f64 };
        out.push_str(&format!(
            "Channel {name}: logged min {min}, max {max}, mean {mean:.3} over {count} rows.\n"
        ));
    }
    out.push_str("\nNo calibration value is emitted.\n");
    out
}

#[tauri::command]
pub fn explain_log_cmd() -> Result<String, String> {
    let samples = logging::get_samples(Some(50_000));
    if samples.is_empty() {
        return Err("No log samples. Import a CSV or capture a session first.".into());
    }
    Ok(explain_log(&samples))
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
        LogSample { timestamp_ms: ms, values, demo: false }
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

    #[test]
    fn change_report_names_overlapping_table_and_refuses_write() {
        let tables = vec![ReportTable {
            name: "Main Spark".into(),
            addr: "0x1".into(),
            rows: 1,
            cols: 2,
            data_type: "UBYTE".into(),
            stride_bytes: 0,
        }];
        let text = change_report(&[0, 1, 2, 3], &[0, 9, 8, 3], &tables);
        assert!(text.contains("Main Spark"), "{text}");
        assert!(text.contains("01 02"), "{text}");
        assert!(text.contains("09 08"), "{text}");
        assert!(text.contains("write_allowed: false"), "{text}");
        assert!(!text.to_lowercase().contains("write enabled"));
    }

    #[test]
    fn explain_log_cites_a_row_and_does_not_suggest_a_calibration() {
        let mut demo = sample(250, 1800.0, 55.0, 4.0);
        demo.demo = true;
        let rows = vec![sample(0, 1000.0, 40.0, 5.0), demo];
        let text = explain_log(&rows);
        assert!(text.contains("Row 0 at 0 ms"), "{text}");
        assert!(text.contains("rpm=1000"), "{text}");
        assert!(text.contains("does not emit a calibration value"), "{text}");
        assert!(text.contains("write_allowed: false"), "{text}");
        assert!(text.contains("demo samples"), "{text}");
        assert!(text.contains("Channel rpm: logged min 1000"), "{text}");
        let low = text.to_lowercase();
        assert!(!low.contains("suggested"), "{text}");
        assert!(!low.contains("set the cell"), "{text}");
    }

    #[test]
    fn change_report_identical_and_length_mismatch() {
        let same = change_report(&[1, 2], &[1, 2], &[]);
        assert!(same.contains("identical"), "{same}");
        let bad = change_report(&[1], &[1, 2], &[]);
        assert!(bad.contains("differ in length"), "{bad}");
        assert!(bad.contains("write_allowed: false"), "{bad}");
    }
}
