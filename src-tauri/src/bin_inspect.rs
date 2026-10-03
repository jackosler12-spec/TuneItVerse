//! Offline BIN inspection. No ECU talk, no write-path changes.
//!
//! Hex window, block entropy, and a markdown tune report so a personal dump
//! can be reviewed before any checksum or flash step.

use serde_json::{json, Value};

const HEX_ROW: usize = 16;
const HEX_MAX: usize = 4096;
const BLOCK: usize = 4096;

fn shannon_entropy(block: &[u8]) -> f64 {
    if block.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for b in block {
        counts[*b as usize] += 1;
    }
    let n = block.len() as f64;
    let mut h = 0.0;
    for c in counts {
        if c == 0 {
            continue;
        }
        let p = c as f64 / n;
        h -= p * p.log2();
    }
    (h * 1000.0).round() / 1000.0
}

fn ascii_byte(b: u8) -> char {
    if (0x20..0x7f).contains(&b) {
        b as char
    } else {
        '.'
    }
}

pub fn hex_window(data: &[u8], offset: usize, length: usize) -> Result<Value, String> {
    if data.is_empty() {
        return Err("No image bytes.".into());
    }
    if offset >= data.len() {
        return Err(format!("offset 0x{:X} past end ({})", offset, data.len()));
    }
    let want = length.clamp(1, HEX_MAX);
    let end = (offset + want).min(data.len());
    let slice = &data[offset..end];
    let mut rows = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < slice.len() {
        let row_end = (i + HEX_ROW).min(slice.len());
        let row = &slice[i..row_end];
        let mut hex = String::new();
        let mut ascii = String::new();
        for (j, b) in row.iter().enumerate() {
            if j > 0 {
                hex.push(' ');
            }
            hex.push_str(&format!("{:02X}", b));
            ascii.push(ascii_byte(*b));
        }
        let addr = offset + i;
        let line = format!("{:06X}: {:<47} | {}", addr, hex, ascii);
        text.push_str(&line);
        text.push('\n');
        rows.push(json!({
            "offset": format!("0x{:06X}", addr),
            "hex": hex,
            "ascii": ascii
        }));
        i += HEX_ROW;
    }
    Ok(json!({
        "offset": offset,
        "length": slice.len(),
        "image_len": data.len(),
        "truncated": end - offset < want,
        "rows": rows,
        "text": text
    }))
}

pub fn profile_bin(data: &[u8]) -> Value {
    if data.is_empty() {
        return json!({"bytes": 0, "note": "empty image"});
    }
    let mut zero = 0usize;
    let mut ff = 0usize;
    let mut printable = 0usize;
    for b in data {
        if *b == 0 {
            zero += 1;
        } else if *b == 0xFF {
            ff += 1;
        }
        if (0x20..0x7f).contains(b) {
            printable += 1;
        }
    }
    let mut blocks = Vec::new();
    let mut high = 0usize;
    let mut flat = 0usize;
    let mut off = 0usize;
    while off < data.len() {
        let end = (off + BLOCK).min(data.len());
        let ent = shannon_entropy(&data[off..end]);
        if ent >= 7.5 {
            high += 1;
        }
        if ent <= 0.05 {
            flat += 1;
        }
        if blocks.len() < 64 || ent >= 7.5 || ent <= 0.05 {
            blocks.push(json!({
                "offset": format!("0x{:06X}", off),
                "length": end - off,
                "entropy": ent
            }));
        }
        off = end;
    }
    let n = data.len() as f64;
    let whole = shannon_entropy(data);
    let note = if whole >= 7.8 {
        "High entropy across the image. Could be compressed or encrypted — do not assume a raw cal."
    } else if zero as f64 / n > 0.5 {
        "Mostly empty (0x00). Typical of an erased or unused region, not a full cal by itself."
    } else if ff as f64 / n > 0.5 {
        "Mostly 0xFF. Typical of erased flash."
    } else {
        "Mixed entropy. Likely a raw calibration or mixed code/cal image. Profile is not an identification."
    };
    json!({
        "bytes": data.len(),
        "entropy": whole,
        "zero_percent": ((zero as f64) * 1000.0 / n).round() / 10.0,
        "ff_percent": ((ff as f64) * 1000.0 / n).round() / 10.0,
        "printable_percent": ((printable as f64) * 1000.0 / n).round() / 10.0,
        "block_bytes": BLOCK,
        "high_entropy_blocks": high,
        "flat_blocks": flat,
        "blocks_sampled": blocks,
        "note": note
    })
}

pub fn tune_report(data: &[u8]) -> Result<String, String> {
    let ident = crate::v29_tools::identify_bin(data);
    let cs = crate::checksum::validate_checksums(data)?;
    let profile = profile_bin(data);
    let mut out = String::new();
    out.push_str("# TuneItVerse tune report\n\n");
    out.push_str(&format!("Tool: TuneItVerse {}\n\n", crate::APP_VERSION));
    out.push_str("Personal dump review only. This report does not enable a write path.\n\n");
    out.push_str("## Identify\n\n");
    out.push_str(&format!("- Bytes: {}\n", data.len()));
    out.push_str(&format!("- Family: {}\n", ident.get("family").and_then(|v| v.as_str()).unwrap_or("unresolved")));
    out.push_str(&format!("- Display: {}\n", ident.get("display_name").and_then(|v| v.as_str()).unwrap_or("—")));
    out.push_str(&format!("- OS hits: {}\n", ident.get("os_id_hits").map(|v| v.to_string()).unwrap_or_else(|| "[]".into())));
    out.push_str(&format!("- Write advertised: {}\n", ident.get("write_allowed").and_then(|v| v.as_bool()).unwrap_or(false)));
    out.push_str(&format!("- Size collision: {}\n\n", ident.get("size_collision").and_then(|v| v.as_bool()).unwrap_or(false)));
    out.push_str("## Checksum\n\n");
    out.push_str(&format!("- Family: {}\n", cs.ecu_family));
    out.push_str(&format!("- Method: {}\n", cs.method_used));
    out.push_str(&format!("- Valid / fixed-needed / failed: {} / {} / {}\n", cs.valid_count, cs.fixed_count, cs.failed_count));
    out.push_str(&format!("- All valid: {}\n\n", cs.all_valid));
    out.push_str("## Profile\n\n");
    out.push_str(&format!("- Entropy: {}\n", profile.get("entropy").unwrap_or(&Value::Null)));
    out.push_str(&format!("- Zero %: {}\n", profile.get("zero_percent").unwrap_or(&Value::Null)));
    out.push_str(&format!("- 0xFF %: {}\n", profile.get("ff_percent").unwrap_or(&Value::Null)));
    out.push_str(&format!("- Note: {}\n", profile.get("note").and_then(|v| v.as_str()).unwrap_or("")));
    Ok(out)
}

#[tauri::command]
pub fn bin_hex_window(data: Vec<u8>, offset: Option<u32>, length: Option<u32>) -> Result<String, String> {
    let off = offset.unwrap_or(0) as usize;
    let len = length.unwrap_or(256) as usize;
    Ok(hex_window(&data, off, len)?.to_string())
}

#[tauri::command]
pub fn bin_profile(data: Vec<u8>) -> Result<String, String> {
    Ok(profile_bin(&data).to_string())
}

#[tauri::command]
pub fn bin_tune_report(data: Vec<u8>) -> Result<String, String> {
    tune_report(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_window_renders_ascii() {
        let mut img = vec![0u8; 32];
        img[0] = b'A';
        img[1] = 0x00;
        img[2] = b'B';
        let v = hex_window(&img, 0, 16).unwrap();
        let text = v["text"].as_str().unwrap();
        assert!(text.contains("41 00 42"));
        assert!(text.contains("A.B"));
    }

    #[test]
    fn hex_window_rejects_past_end() {
        assert!(hex_window(&[1, 2, 3], 9, 16).is_err());
    }

    #[test]
    fn blank_block_is_flat() {
        let img = vec![0u8; 8192];
        let p = profile_bin(&img);
        assert_eq!(p["entropy"], 0.0);
        assert!(p["flat_blocks"].as_u64().unwrap() >= 2);
        assert!(p["note"].as_str().unwrap().contains("empty"));
    }

    #[test]
    fn mixed_bytes_are_not_called_encrypted() {
        let mut img = vec![0u8; 256];
        for (i, b) in img.iter_mut().enumerate() {
            *b = (i % 17) as u8;
        }
        let p = profile_bin(&img);
        let ent = p["entropy"].as_f64().unwrap();
        assert!(ent > 1.0 && ent < 7.8);
    }

    #[test]
    fn report_on_unknown_size_stays_honest() {
        let text = tune_report(&vec![1u8, 2, 3, 4]).unwrap();
        assert!(text.contains("does not enable a write path"));
        assert!(text.contains("report-only") || text.contains("UNKNOWN") || text.contains("unresolved"));
    }
}
