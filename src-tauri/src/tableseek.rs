//! Universal Patcher TableSeek — locate P01/P59 tables by byte pattern.
//! SearchStr `@` bytes are the table address (big-endian). No hit → skip (do not invent).

use crate::xdf::TableDef;
use serde::Deserialize;

const P01_TABLESEEK_XML: &str = include_str!("../../reference/tableseek-p01-p59.xml");

#[derive(Debug, Deserialize, Default)]
struct ArrayOfTableSeek {
    #[serde(rename = "TableSeek", default)]
    table_seek: Vec<TableSeekXml>,
}

#[derive(Debug, Deserialize, Default)]
struct TableSeekXml {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "SearchStr", default)]
    search_str: String,
    #[serde(rename = "Rows", default)]
    rows: String,
    #[serde(rename = "Columns", default)]
    cols: String,
    #[serde(rename = "RowHeaders", default)]
    row_headers: String,
    #[serde(rename = "ColHeaders", default)]
    col_headers: String,
    #[serde(rename = "Math", default)]
    math: String,
    #[serde(rename = "Offset", default)]
    offset: String,
    #[serde(rename = "DataType", default)]
    data_type: String,
    #[serde(rename = "UseHit", default)]
    use_hit: String,
    #[serde(rename = "Category", default)]
    category: String,
    #[serde(rename = "Units", default)]
    units: String,
    #[serde(rename = "RowMajor", default)]
    row_major: String,
    #[serde(rename = "MSB", default)]
    msb: String,
    #[serde(rename = "RefAddress", default)]
    ref_address: String,
    #[serde(rename = "Description", default)]
    description: String,
    #[serde(rename = "Decimals", default)]
    decimals: String,
    #[serde(rename = "SignedOffset", default)]
    signed_offset: String,
}

#[derive(Debug, Clone)]
enum PatTok {
    Byte(u8),
    Wild,
}

fn parse_usize(s: &str, default: usize) -> usize {
    let t = s.trim();
    if t.is_empty() { return default; }
    t.parse().unwrap_or(default)
}

fn parse_i32(s: &str) -> i32 {
    s.trim().parse().unwrap_or(0)
}

fn is_true(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

fn parse_search_pattern(s: &str) -> Vec<PatTok> {
    s.split_whitespace()
        .filter_map(|tok| {
            let t = tok.trim();
            if t.is_empty() { return None; }
            if t == "@" || t == "??" || t == "?" {
                Some(PatTok::Wild)
            } else {
                u8::from_str_radix(t.trim_start_matches("0x").trim_start_matches("0X"), 16)
                    .ok()
                    .map(PatTok::Byte)
            }
        })
        .collect()
}

fn pattern_matches(hay: &[u8], pat: &[PatTok]) -> bool {
    if hay.len() < pat.len() { return false; }
    pat.iter().zip(hay.iter()).all(|(p, &b)| match p {
        PatTok::Wild => true,
        PatTok::Byte(x) => *x == b,
    })
}

/// Returns start index of each match.
fn find_pattern_hits(bin: &[u8], pat: &[PatTok]) -> Vec<usize> {
    if pat.is_empty() || bin.len() < pat.len() { return vec![]; }
    let last = bin.len() - pat.len();
    let first = match pat.first() {
        Some(PatTok::Byte(b)) => Some(*b),
        _ => None,
    };
    let mut hits = Vec::new();
    let mut i = 0;
    while i <= last {
        if let Some(b) = first {
            if bin[i] != b {
                i += 1;
                continue;
            }
        }
        if pattern_matches(&bin[i..], pat) {
            hits.push(i);
        }
        i += 1;
    }
    hits
}

/// Address from `@` bytes (big-endian). No wildcards → match index.
fn address_from_hit(bin: &[u8], hit: usize, pat: &[PatTok], offset: i32) -> Option<usize> {
    if hit + pat.len() > bin.len() { return None; }
    let mut wild: Vec<u8> = Vec::new();
    for (k, tok) in pat.iter().enumerate() {
        if matches!(tok, PatTok::Wild) {
            wild.push(bin[hit + k]);
        }
    }
    let mut addr: u32 = if wild.is_empty() {
        hit as u32
    } else {
        let mut v: u32 = 0;
        for b in &wild {
            v = (v << 8) | (*b as u32);
        }
        v
    };
    if offset < 0 {
        addr = addr.wrapping_sub((-offset) as u32);
    } else {
        addr = addr.wrapping_add(offset as u32);
    }
    Some(addr as usize)
}

fn parse_tableseek_xml(xml: &str) -> Result<Vec<TableSeekXml>, String> {
    let root: ArrayOfTableSeek = quick_xml::de::from_str(xml)
        .map_err(|e| format!("TableSeek XML parse: {}", e))?;
    Ok(root.table_seek)
}

pub fn tableseek_pack_count() -> usize {
    parse_tableseek_xml(P01_TABLESEEK_XML).map(|v| v.len()).unwrap_or(0)
}

fn sanitize_id(name: &str, i: usize) -> String {
    let mut s = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>();
    if s.is_empty() { s = format!("table_{}", i); }
    s
}

pub fn locate_p01_tables(bin: &[u8]) -> (Vec<TableDef>, usize, usize) {
    let seeks = match parse_tableseek_xml(P01_TABLESEEK_XML) {
        Ok(v) => v,
        Err(_) => return (vec![], 0, 0),
    };
    let total = seeks.len();
    let mut tables = Vec::with_capacity(total);
    let mut seen = std::collections::HashSet::new();
    for (i, t) in seeks.iter().enumerate() {
        if let Some(mut def) = seek_to_def_local(bin, t, i) {
            if seen.insert(def.id.clone()) {
                tables.push(def);
            } else {
                def.id = format!("{}_{}", def.id, i);
                tables.push(def);
            }
        }
    }
    let located = tables.len();
    let missing = total.saturating_sub(located);
    (tables, located, missing)
}

fn seek_to_def_local(bin: &[u8], t: &TableSeekXml, idx: usize) -> Option<TableDef> {
    if t.name.trim().is_empty() { return None; }
    let pat = parse_search_pattern(&t.search_str);
    let use_hit = parse_usize(&t.use_hit, 1).max(1);
    let offset = parse_i32(&t.offset);
    let addr = if pat.is_empty() {
        None
    } else {
        let hits = find_pattern_hits(bin, &pat);
        hits.get(use_hit - 1).and_then(|&h| address_from_hit(bin, h, &pat, offset))
    };
    let addr = addr?;
    if addr >= bin.len() { return None; }
    let rows = parse_usize(&t.rows, 1).max(1);
    let cols = parse_usize(&t.cols, 1).max(1);
    Some(TableDef {
        id: sanitize_id(&t.name, idx),
        name: t.name.clone(),
        category: if t.category.trim().is_empty() { None } else { Some(t.category.clone()) },
        description: t.description.clone(),
        rows,
        cols,
        addr: format!("0x{:06X}", addr),
        data_type: if t.data_type.trim().is_empty() { "UBYTE".into() } else { t.data_type.clone() },
        math: if t.math.trim().is_empty() { "X".into() } else { t.math.clone() },
        units: t.units.clone(),
        row_major: t.row_major.trim().is_empty() || is_true(&t.row_major),
        msb: t.msb.trim().is_empty() || is_true(&t.msb),
        row_headers: if t.row_headers.trim().is_empty() { None } else { Some(t.row_headers.clone()) },
        col_headers: if t.col_headers.trim().is_empty() { None } else { Some(t.col_headers.clone()) },
        decimals: t.decimals.trim().parse().unwrap_or(2),
        file_offset: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_has_about_1500() {
        let n = tableseek_pack_count();
        assert!(n >= 1500, "expected ~1598 TableSeek entries, got {}", n);
    }

    #[test]
    fn pattern_wildcard_address() {
        // FF FA 16 39 @ @ @ @ E7 0B
        let mut bin = vec![0u8; 32];
        bin[4..14].copy_from_slice(&[0xFF, 0xFA, 0x16, 0x39, 0x00, 0x00, 0x80, 0x21, 0xE7, 0x0B]);
        let pat = parse_search_pattern("FF FA 16 39 @ @ @ @ E7 0B");
        let hits = find_pattern_hits(&bin, &pat);
        assert_eq!(hits, vec![4]);
        let addr = address_from_hit(&bin, hits[0], &pat, 0).unwrap();
        assert_eq!(addr, 0x00008021);
    }

    #[test]
    fn empty_bin_locates_none() {
        let (t, loc, miss) = locate_p01_tables(&[]);
        assert!(t.is_empty());
        assert_eq!(loc, 0);
        assert!(miss >= 1500);
    }
    #[test]
    fn locates_many_on_reference_p01_bin() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../reference/LS1 PCM P01 12225074.bin");
        let data = std::fs::read(path).expect("reference P01 bin");
        let r = crate::ecu_database::tables_for_bin(&data);
        assert!(r.located >= 200, "expected hundreds of TableSeek hits on 12225074, got {} ({})", r.located, r.note);
    }
}
