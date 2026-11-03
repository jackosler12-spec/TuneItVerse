//! Live dashboard data: channel stats, recent series and log-vs-map-cell hit
//! counts. Read-only. Works on whatever is in the logger buffer (live Mode 01
//! samples, imported CSV, or explicit demo samples, which are flagged).
//! Never writes to a BIN or an ECU and never invents channel meanings.

use crate::logging::{self, LogSample};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Index of the nearest breakpoint. Axis must be non-empty and finite.
/// Returns None when `v` lies outside the axis by more than half the edge step,
/// so out-of-range samples are reported instead of piled into the edge cell.
pub fn nearest_index(axis: &[f64], v: f64) -> Option<usize> {
    if axis.is_empty() || !v.is_finite() || axis.iter().any(|a| !a.is_finite()) {
        return None;
    }
    if axis.len() == 1 {
        return Some(0);
    }
    let asc = axis[axis.len() - 1] >= axis[0];
    let (lo, hi) = if asc { (axis[0], axis[axis.len() - 1]) } else { (axis[axis.len() - 1], axis[0]) };
    let step_lo = (axis[1] - axis[0]).abs() / 2.0;
    let n = axis.len();
    let step_hi = (axis[n - 1] - axis[n - 2]).abs() / 2.0;
    let (slack_lo, slack_hi) = if asc { (step_lo, step_hi) } else { (step_hi, step_lo) };
    if v < lo - slack_lo || v > hi + slack_hi {
        return None;
    }
    let mut best = 0usize;
    let mut bd = f64::INFINITY;
    for (i, a) in axis.iter().enumerate() {
        let d = (a - v).abs();
        if d < bd {
            bd = d;
            best = i;
        }
    }
    Some(best)
}

fn series_stats(samples: &[LogSample]) -> BTreeMap<String, Value> {
    let mut acc: BTreeMap<String, (f64, f64, f64, usize, f64)> = BTreeMap::new();
    for s in samples {
        for (k, v) in &s.values {
            if !v.is_finite() {
                continue;
            }
            let e = acc.entry(k.clone()).or_insert((f64::INFINITY, f64::NEG_INFINITY, 0.0, 0, 0.0));
            e.0 = e.0.min(*v);
            e.1 = e.1.max(*v);
            e.2 += v;
            e.3 += 1;
            e.4 = *v;
        }
    }
    acc.into_iter()
        .map(|(k, (mn, mx, sum, n, last))| {
            (k, json!({"min": mn, "max": mx, "mean": sum / n as f64, "count": n, "last": last}))
        })
        .collect()
}

/// Build the snapshot from a sample slice (pure; tested without the global buffer).
pub fn build_snapshot(
    samples: &[LogSample],
    series_len: usize,
    x_channel: Option<&str>,
    y_channel: Option<&str>,
    z_channel: Option<&str>,
    x_axis: &[f64],
    y_axis: &[f64],
) -> Value {
    let demo = samples.iter().any(|s| s.demo);
    let stats = series_stats(samples);
    let start = samples.len().saturating_sub(series_len.max(2));
    let recent: Vec<Value> = samples[start..]
        .iter()
        .map(|s| json!({"t": s.timestamp_ms, "v": s.values}))
        .collect();

    let mut cells = Value::Null;
    let mut scatter: Vec<Value> = Vec::new();
    if let (Some(xc), Some(yc)) = (x_channel, y_channel) {
        let rows = y_axis.len().max(1);
        let cols = x_axis.len();
        let mut hits = vec![vec![0u32; cols]; rows];
        let mut zsum = vec![vec![0f64; cols]; rows];
        let mut zn = vec![vec![0u32; cols]; rows];
        let (mut outside, mut missing) = (0usize, 0usize);
        for s in samples {
            let (Some(x), Some(y)) = (s.values.get(xc).copied(), s.values.get(yc).copied()) else {
                missing += 1;
                continue;
            };
            if scatter.len() < 4000 {
                scatter.push(json!([x, y]));
            }
            if cols == 0 {
                continue;
            }
            let ci = nearest_index(x_axis, x);
            let ri = if y_axis.is_empty() { Some(0) } else { nearest_index(y_axis, y) };
            match (ri, ci) {
                (Some(r), Some(c)) => {
                    hits[r][c] += 1;
                    if let Some(z) = z_channel.and_then(|zc| s.values.get(zc)).copied().filter(|z| z.is_finite()) {
                        zsum[r][c] += z;
                        zn[r][c] += 1;
                    }
                }
                _ => outside += 1,
            }
        }
        if cols > 0 {
            let zmean: Vec<Vec<Value>> = (0..rows)
                .map(|r| (0..cols).map(|c| if zn[r][c] > 0 { json!(zsum[r][c] / zn[r][c] as f64) } else { Value::Null }).collect())
                .collect();
            cells = json!({"rows": rows, "cols": cols, "hits": hits, "z_mean": zmean,
                           "outside_axis": outside, "missing_channel": missing});
        } else {
            cells = json!({"rows": 0, "cols": 0, "missing_channel": missing,
                           "note": "No numeric axis breakpoints for the selected table; scatter only."});
        }
    }

    json!({
        "count": samples.len(),
        "demo": demo,
        "stats": stats,
        "recent": recent,
        "scatter": scatter,
        "cells": cells,
        "x_channel": x_channel, "y_channel": y_channel, "z_channel": z_channel,
        "write_allowed": false,
        "note": "Read-only view of the logger buffer. Not hardware-tested. Channel names come from the log; no meanings are invented."
    })
}

/// Parse a comma/space separated header string into numbers. Any non-numeric
/// entry makes the whole axis unusable (returns empty) rather than guessing.
pub fn parse_axis(s: &str) -> Vec<f64> {
    let parts: Vec<&str> = s.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()).collect();
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        match p.parse::<f64>() {
            Ok(v) if v.is_finite() => out.push(v),
            _ => return Vec::new(),
        }
    }
    out
}

#[tauri::command]
pub fn dashboard_snapshot(
    series_len: Option<usize>,
    x_channel: Option<String>,
    y_channel: Option<String>,
    z_channel: Option<String>,
    x_headers: Option<String>,
    y_headers: Option<String>,
) -> Result<String, String> {
    let samples = logging::get_samples(Some(50_000));
    let xa = x_headers.as_deref().map(parse_axis).unwrap_or_default();
    let ya = y_headers.as_deref().map(parse_axis).unwrap_or_default();
    Ok(build_snapshot(
        &samples,
        series_len.unwrap_or(300).min(5000),
        x_channel.as_deref().filter(|s| !s.is_empty()),
        y_channel.as_deref().filter(|s| !s.is_empty()),
        z_channel.as_deref().filter(|s| !s.is_empty()),
        &xa,
        &ya,
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn s(t: u64, rpm: f64, map: f64, stft: f64) -> LogSample {
        let mut v = HashMap::new();
        v.insert("rpm".into(), rpm);
        v.insert("map".into(), map);
        v.insert("stft".into(), stft);
        LogSample { timestamp_ms: t, values: v, demo: false }
    }

    #[test]
    fn nearest_index_bounds() {
        let a = [800.0, 1600.0, 2400.0];
        assert_eq!(nearest_index(&a, 900.0), Some(0));
        assert_eq!(nearest_index(&a, 2100.0), Some(2));
        assert_eq!(nearest_index(&a, 2700.0), Some(2));
        assert_eq!(nearest_index(&a, 3000.0), None);
        assert_eq!(nearest_index(&a, 300.0), None);
        assert_eq!(nearest_index(&[100.0, 50.0, 0.0], 45.0), Some(1));
        assert_eq!(nearest_index(&a, f64::NAN), None);
    }

    #[test]
    fn parse_axis_fails_closed() {
        assert_eq!(parse_axis("800,1600, 2400"), vec![800.0, 1600.0, 2400.0]);
        assert!(parse_axis("800,abc").is_empty());
        assert!(parse_axis("").is_empty());
    }

    #[test]
    fn snapshot_bins_hits_and_reports_outside() {
        let samples = vec![s(0, 800.0, 20.0, 2.0), s(100, 1650.0, 95.0, -4.0), s(200, 1600.0, 100.0, -2.0), s(300, 9000.0, 20.0, 0.0)];
        let v = build_snapshot(&samples, 300, Some("rpm"), Some("map"), Some("stft"), &[800.0, 1600.0, 2400.0], &[20.0, 100.0]);
        assert_eq!(v["write_allowed"], false);
        assert_eq!(v["cells"]["hits"][0][0], 1);
        assert_eq!(v["cells"]["hits"][1][1], 2);
        assert_eq!(v["cells"]["z_mean"][1][1], -3.0);
        assert_eq!(v["cells"]["outside_axis"], 1);
        assert!(v["cells"]["z_mean"][0][2].is_null());
        assert_eq!(v["stats"]["rpm"]["max"], 9000.0);
        assert_eq!(v["scatter"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn snapshot_missing_channel_and_no_axes() {
        let samples = vec![s(0, 800.0, 20.0, 2.0)];
        let v = build_snapshot(&samples, 10, Some("rpm"), Some("boost"), None, &[], &[]);
        assert_eq!(v["cells"]["missing_channel"], 1);
        let v = build_snapshot(&samples, 10, Some("rpm"), Some("map"), None, &[], &[]);
        assert_eq!(v["cells"]["cols"], 0);
        assert_eq!(v["recent"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn snapshot_flags_demo() {
        let mut d = s(0, 800.0, 20.0, 0.0);
        d.demo = true;
        assert_eq!(build_snapshot(&[d], 10, None, None, None, &[], &[])["demo"], true);
    }
}
