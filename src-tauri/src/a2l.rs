//! Lightweight ASAM A2L scanner.
//! Reads CHARACTERISTIC / MEASUREMENT name + address + conversion hints from
//! a personal A2L. Not a full ASAP2 compiler.

use crate::xdf::TableDef;
use serde_json::json;

fn take_ident(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
    Some((&s[..end], s[end..].trim_start()))
}

fn parse_addr_token(tok: &str) -> Option<String> {
    let t = tok.trim().trim_start_matches("0x").trim_start_matches("0X");
    if t.chars().all(|c| c.is_ascii_hexdigit()) && !t.is_empty() {
        Some(format!("0x{}", t.to_ascii_uppercase()))
    } else {
        None
    }
}

pub fn parse_a2l(text: &str) -> Vec<TableDef> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim();
        let up = t.to_ascii_uppercase();
        if !up.contains("/BEGIN CHARACTERISTIC") && !up.contains("/BEGIN MEASUREMENT") {
            continue;
        }
        let after = if let Some(idx) = up.find("CHARACTERISTIC") {
            t[idx + "CHARACTERISTIC".len()..].trim()
        } else if let Some(idx) = up.find("MEASUREMENT") {
            t[idx + "MEASUREMENT".len()..].trim()
        } else {
            ""
        };
        let mut name = String::new();
        let mut rest = after;
        if let Some((n, r)) = take_ident(after) {
            if !n.is_empty() && !n.starts_with('"') { name = n.to_string(); rest = r; }
        }
        let mut desc = String::new();
        if rest.starts_with('"') {
            if let Some(end) = rest[1..].find('"') {
                desc = rest[1..=end].to_string();
            }
        }
        let mut addr = String::new();
        let mut rows = 1usize;
        let mut cols = 1usize;
        let mut math = "X".to_string();
        let mut units = String::new();
        let mut kind = "VALUE";
        for _ in 0..80 {
            let Some(l) = lines.next() else { break; };
            let s = l.trim();
            let su = s.to_ascii_uppercase();
            if su.contains("/END CHARACTERISTIC") || su.contains("/END MEASUREMENT") { break; }
            if su.contains("/BEGIN AXIS_DESCR") || su.contains("/BEGIN AXIS_PTS") {
                if rows == 1 { rows = 16; } else if cols == 1 { cols = 16; }
            }
            if name.is_empty() {
                if let Some((n, _)) = take_ident(s) {
                    if n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && n.len() > 1 {
                        name = n.to_string();
                    }
                }
            }
            for tok in s.split_whitespace() {
                if addr.is_empty() {
                    if let Some(a) = parse_addr_token(tok) {
                        if a.len() >= 6 { addr = a; }
                    }
                }
                if tok.eq_ignore_ascii_case("MAP") { kind = "MAP"; if rows == 1 { rows = 16; } if cols == 1 { cols = 16; } }
                if tok.eq_ignore_ascii_case("CURVE") { kind = "CURVE"; if cols == 1 { cols = 16; } }
            }
            if su.starts_with("FORMAT") || su.starts_with("COMPU_METHOD") {
                if s.contains('*') { math = s.to_string(); }
            }
            if su.starts_with("UNIT") || su.starts_with("PHYS_UNIT") {
                if let Some(q) = s.find('"') {
                    if let Some(e) = s[q + 1..].find('"') {
                        units = s[q + 1..q + 1 + e].to_string();
                    }
                }
            }
        }
        if name.is_empty() { continue; }
        if addr.is_empty() { addr = "0x00000000".into(); }
        out.push(TableDef {
            id: name.to_lowercase(),
            name: name.clone(),
            category: Some(format!("A2L {}", kind)),
            description: if desc.is_empty() { format!("A2L {}", name) } else { desc },
            rows,
            cols,
            addr,
            data_type: "UWORD".into(),
            math,
            units,
            row_major: true,
            msb: true,
        });
        if out.len() >= 256 { break; }
    }
    out
}

#[tauri::command]
pub fn parse_a2l_definitions(text: String) -> Result<Vec<TableDef>, String> {
    Ok(parse_a2l(&text))
}

#[tauri::command]
pub fn parse_a2l_summary(text: String) -> Result<String, String> {
    let defs = parse_a2l(&text);
    Ok(json!({
        "count": defs.len(),
        "names": defs.iter().take(40).map(|d| d.name.clone()).collect::<Vec<_>>(),
        "notes": "Addresses and axis sizes are hints from a personal A2L. Confirm on your dump before write."
    }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_characteristic() {
        let a2l = r#"
        /begin CHARACTERISTIC VE_MAIN "Main volumetric efficiency" MAP 0x4000
          /begin AXIS_DESCR
          /end AXIS_DESCR
          /begin AXIS_DESCR
          /end AXIS_DESCR
          PHYS_UNIT "%"
        /end CHARACTERISTIC
        "#;
        let defs = parse_a2l(a2l);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "VE_MAIN");
        assert!(defs[0].addr.to_ascii_uppercase().contains("4000"));
        assert!(defs[0].rows >= 16);
    }
}
