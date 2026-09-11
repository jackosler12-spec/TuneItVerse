//! Universal Patcher TableSeek — locate P01/P59 tables by byte pattern.
//!
//! SearchStr tokens (same rules as UniversalPatcher `TableSeek.cs`):
//! - hex byte = literal
//! - `*` / `?` / `??` = wildcard (not part of the address when `@` is present)
//! - `@` = address byte (big-endian, 2 or 4)
//! - `+D12` / `+W6` = after the hit, read a 32/16-bit pointer at addr+N
//!
//! No hit → skip. Do not invent addresses.

use crate::xdf::TableDef;
use serde::Deserialize;
use std::sync::OnceLock;

const P01_TABLESEEK_XML: &str = include_str!("../../reference/tableseek-p01-p59.xml");

#[derive(Debug, Deserialize, Default)]
struct ArrayOfTableSeek {
    #[serde(rename = "TableSeek", default)]
    table_seek: Vec<TableSeekXml>,
}

#[derive(Debug, Deserialize, Default, Clone)]
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
    #[serde(rename = "Description", default)]
    description: String,
    #[serde(rename = "Decimals", default)]
    decimals: String,
    #[serde(rename = "SignedOffset", default)]
    signed_offset: String,
    #[serde(rename = "ConditionalOffset", default)]
    conditional_offset: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatTok {
    Byte(u8),
    Wild,
    Addr,
}

#[derive(Debug, Clone, Copy)]
struct Jump {
    offset: i32,
    dword: bool,
}

struct CompiledSeek {
    idx: usize,
    name: String,
    category: String,
    description: String,
    rows: usize,
    cols: usize,
    data_type: String,
    math: String,
    units: String,
    row_headers: String,
    col_headers: String,
    decimals: u8,
    row_major: bool,
    msb: bool,
    offset: i32,
    use_hit: usize,
    signed_offset: bool,
    conditional_offset: bool,
    pat: Vec<PatTok>,
    jumps: Vec<Jump>,
    first_lit: Option<(usize, u8)>,
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

fn parse_jump_token(t: &str) -> Option<Jump> {
    let t = t.trim();
    if t.len() < 2 { return None; }
    let sign: i32 = if t.starts_with('+') {
        1
    } else if t.starts_with('-') {
        let rest = &t.as_bytes()[1..];
        if rest.is_empty() || !rest[0].is_ascii_alphabetic() { return None; }
        -1
    } else {
        return None;
    };
    let rest = &t[1..];
    let up = rest.to_ascii_uppercase();
    let dword = up.contains('D');
    let num: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    let offset = num.parse::<i32>().unwrap_or(0) * sign;
    Some(Jump { offset, dword })
}

fn parse_search_pattern(s: &str) -> (Vec<PatTok>, Vec<Jump>) {
    let mut pat = Vec::new();
    let mut jumps = Vec::new();
    for tok in s.split_whitespace() {
        let t = tok.trim();
        if t.is_empty() { continue; }
        if let Some(j) = parse_jump_token(t) {
            jumps.push(j);
            continue;
        }
        if t == "@" {
            pat.push(PatTok::Addr);
        } else if t == "*" || t == "?" || t == "??" {
            pat.push(PatTok::Wild);
        } else {
            let hex: String = t.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() == 1 || hex.len() == 2 {
                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                    pat.push(PatTok::Byte(b));
                }
            }
        }
    }
    (pat, jumps)
}

fn first_literal(pat: &[PatTok]) -> Option<(usize, u8)> {
    pat.iter().enumerate().find_map(|(i, t)| match t {
        PatTok::Byte(b) => Some((i, *b)),
        _ => None,
    })
}

fn pattern_matches(hay: &[u8], pat: &[PatTok]) -> bool {
    if hay.len() < pat.len() { return false; }
    pat.iter().zip(hay.iter()).all(|(p, &b)| match p {
        PatTok::Byte(x) => *x == b,
        PatTok::Wild | PatTok::Addr => true,
    })
}

fn read_u16_be(bin: &[u8], off: usize) -> Option<u32> {
    if off + 2 > bin.len() { return None; }
    Some(((bin[off] as u32) << 8) | bin[off + 1] as u32)
}

fn read_u32_be(bin: &[u8], off: usize) -> Option<u32> {
    if off + 4 > bin.len() { return None; }
    Some(
        ((bin[off] as u32) << 24)
            | ((bin[off + 1] as u32) << 16)
            | ((bin[off + 2] as u32) << 8)
            | bin[off + 3] as u32,
    )
}

fn be_from_bytes(bytes: &[u8]) -> Option<u32> {
    if bytes.len() >= 4 {
        Some(
            ((bytes[0] as u32) << 24)
                | ((bytes[1] as u32) << 16)
                | ((bytes[2] as u32) << 8)
                | bytes[3] as u32,
        )
    } else if bytes.len() >= 2 {
        Some(((bytes[0] as u32) << 8) | bytes[1] as u32)
    } else {
        None
    }
}

/// Address from `@` (preferred) or `*` bytes, then Offset, then +D/+W pointer follows.
fn address_from_hit(bin: &[u8], hit: usize, seek: &CompiledSeek) -> Option<usize> {
    let pat = &seek.pat;
    if hit + pat.len() > bin.len() { return None; }
    let mut addr_bytes: Vec<u8> = Vec::new();
    let mut wild_bytes: Vec<u8> = Vec::new();
    for (k, tok) in pat.iter().enumerate() {
        match tok {
            PatTok::Addr => addr_bytes.push(bin[hit + k]),
            PatTok::Wild => wild_bytes.push(bin[hit + k]),
            PatTok::Byte(_) => {}
        }
    }
    let mut addr: u32 = if let Some(v) = be_from_bytes(&addr_bytes) {
        v
    } else if addr_bytes.is_empty() {
        if let Some(v) = be_from_bytes(&wild_bytes) {
            v
        } else {
            read_u32_be(bin, hit + pat.len())?
        }
    } else {
        return None;
    };

    if seek.conditional_offset || seek.signed_offset {
        let low = (addr & 0xFFFF) as u16;
        if seek.conditional_offset && low > 0x5000 {
            addr = addr.wrapping_sub(0x10000);
        }
        if seek.signed_offset && low > 0x8000 {
            addr = addr.wrapping_sub(0x10000);
        }
    }

    if seek.offset < 0 {
        addr = addr.wrapping_sub((-seek.offset) as u32);
    } else {
        addr = addr.wrapping_add(seek.offset as u32);
    }

    for j in &seek.jumps {
        let loc = if j.offset < 0 {
            addr.wrapping_sub((-j.offset) as u32) as usize
        } else {
            addr.wrapping_add(j.offset as u32) as usize
        };
        addr = if j.dword {
            read_u32_be(bin, loc)?
        } else {
            read_u16_be(bin, loc)?
        };
    }

    Some(addr as usize)
}

fn find_nth_hit(bin: &[u8], pat: &[PatTok], first_lit: Option<(usize, u8)>, n: usize, index: &[Vec<usize>]) -> Option<usize> {
    if pat.is_empty() || bin.len() < pat.len() || n == 0 { return None; }
    let mut found = 0usize;
    if let Some((lit_off, lit_b)) = first_lit {
        for &pos in &index[lit_b as usize] {
            if pos < lit_off { continue; }
            let hit = pos - lit_off;
            if hit + pat.len() > bin.len() { continue; }
            if pattern_matches(&bin[hit..], pat) {
                found += 1;
                if found == n { return Some(hit); }
            }
        }
        None
    } else {
        let last = bin.len() - pat.len();
        for i in 0..=last {
            if pattern_matches(&bin[i..], pat) {
                found += 1;
                if found == n { return Some(i); }
            }
        }
        None
    }
}

fn parse_tableseek_xml(xml: &str) -> Result<Vec<TableSeekXml>, String> {
    let root: ArrayOfTableSeek = quick_xml::de::from_str(xml)
        .map_err(|e| format!("TableSeek XML parse: {}", e))?;
    Ok(root.table_seek)
}

fn compile_seek(t: TableSeekXml, idx: usize) -> Option<CompiledSeek> {
    if t.name.trim().is_empty() { return None; }
    let (pat, jumps) = parse_search_pattern(&t.search_str);
    if pat.is_empty() { return None; }
    Some(CompiledSeek {
        idx,
        name: t.name,
        category: t.category,
        description: t.description,
        rows: parse_usize(&t.rows, 1).max(1),
        cols: parse_usize(&t.cols, 1).max(1),
        data_type: if t.data_type.trim().is_empty() { "UBYTE".into() } else { t.data_type },
        math: if t.math.trim().is_empty() { "X".into() } else { t.math },
        units: t.units,
        row_headers: t.row_headers,
        col_headers: t.col_headers,
        decimals: t.decimals.trim().parse().unwrap_or(2),
        row_major: t.row_major.trim().is_empty() || is_true(&t.row_major),
        msb: t.msb.trim().is_empty() || is_true(&t.msb),
        offset: parse_i32(&t.offset),
        use_hit: parse_usize(&t.use_hit, 1).max(1),
        signed_offset: is_true(&t.signed_offset),
        conditional_offset: is_true(&t.conditional_offset),
        first_lit: first_literal(&pat),
        pat,
        jumps,
    })
}

fn pack() -> &'static [CompiledSeek] {
    static PACK: OnceLock<Vec<CompiledSeek>> = OnceLock::new();
    PACK.get_or_init(|| {
        parse_tableseek_xml(P01_TABLESEEK_XML)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .filter_map(|(i, t)| compile_seek(t, i))
            .collect()
    })
}

pub fn tableseek_pack_count() -> usize {
    pack().len()
}

fn sanitize_id(name: &str, i: usize) -> String {
    let mut s = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>();
    if s.is_empty() { s = format!("table_{}", i); }
    s
}

fn seek_to_def(bin: &[u8], t: &CompiledSeek, index: &[Vec<usize>]) -> Option<TableDef> {
    let hit = find_nth_hit(bin, &t.pat, t.first_lit, t.use_hit, index)?;
    let addr = address_from_hit(bin, hit, t)?;
    if addr == 0 || addr >= bin.len() { return None; }
    Some(TableDef {
        id: sanitize_id(&t.name, t.idx),
        name: t.name.clone(),
        category: if t.category.trim().is_empty() { None } else { Some(t.category.clone()) },
        description: t.description.clone(),
        rows: t.rows,
        cols: t.cols,
        addr: format!("0x{:06X}", addr),
        data_type: t.data_type.clone(),
        math: t.math.clone(),
        units: t.units.clone(),
        row_major: t.row_major,
        msb: t.msb,
        row_headers: if t.row_headers.trim().is_empty() { None } else { Some(t.row_headers.clone()) },
        col_headers: if t.col_headers.trim().is_empty() { None } else { Some(t.col_headers.clone()) },
        decimals: t.decimals,
        file_offset: true,
    })
}

fn build_byte_index(bin: &[u8], seeks: &[CompiledSeek]) -> Vec<Vec<usize>> {
    let mut want = [false; 256];
    for s in seeks {
        if let Some((_, b)) = s.first_lit {
            want[b as usize] = true;
        }
    }
    let mut index = vec![Vec::new(); 256];
    for (i, &b) in bin.iter().enumerate() {
        if want[b as usize] {
            index[b as usize].push(i);
        }
    }
    index
}

pub fn locate_p01_tables(bin: &[u8]) -> (Vec<TableDef>, usize, usize) {
    let seeks = pack();
    let total = seeks.len();
    if bin.is_empty() {
        return (vec![], 0, total);
    }
    let index = build_byte_index(bin, seeks);
    let mut tables = Vec::with_capacity(total);
    let mut seen = std::collections::HashSet::new();
    for t in seeks {
        if let Some(mut def) = seek_to_def(bin, t, &index) {
            if !seen.insert(def.id.clone()) {
                def.id = format!("{}_{}", def.id, t.idx);
            }
            tables.push(def);
        }
    }
    let located = tables.len();
    let missing = total.saturating_sub(located);
    (tables, located, missing)
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
        let mut bin = vec![0u8; 32];
        bin[4..14].copy_from_slice(&[0xFF, 0xFA, 0x16, 0x39, 0x00, 0x00, 0x80, 0x21, 0xE7, 0x0B]);
        let (pat, jumps) = parse_search_pattern("FF FA 16 39 @ @ @ @ E7 0B");
        assert!(jumps.is_empty());
        assert!(pattern_matches(&bin[4..], &pat));
        let seek = compile_seek(TableSeekXml {
            name: "t".into(),
            search_str: "FF FA 16 39 @ @ @ @ E7 0B".into(),
            ..Default::default()
        }, 0).unwrap();
        let addr = address_from_hit(&bin, 4, &seek).unwrap();
        assert_eq!(addr, 0x00008021);
    }

    #[test]
    fn star_wildcard_is_not_address_when_at_present() {
        // 11 C3 * * 42 43 16 39 @ @ @ @  → address from @ only
        let mut bin = vec![0u8; 32];
        bin[2..16].copy_from_slice(&[
            0x11, 0xC3, 0xAA, 0xBB, 0x42, 0x43, 0x16, 0x39,
            0x00, 0x00, 0x80, 0x21, 0x00, 0x00,
        ]);
        let seek = compile_seek(TableSeekXml {
            name: "Engine_Identifier".into(),
            search_str: "11 C3 * * 42 43 16 39 @ @ @ @".into(),
            ..Default::default()
        }, 0).unwrap();
        let addr = address_from_hit(&bin, 2, &seek).unwrap();
        assert_eq!(addr, 0x00008021);
        assert_ne!(addr, 0xAABB0000);
    }

    #[test]
    fn plus_d_pointer_follow() {
        let mut bin = vec![0u8; 64];
        // FF @ @ @ @ AA +D4  at offset 4; @@@@ = 0x00000020; dword at 0x24 = 0x00000030
        bin[4..10].copy_from_slice(&[0xFF, 0x00, 0x00, 0x00, 0x20, 0xAA]);
        bin[0x24..0x28].copy_from_slice(&[0x00, 0x00, 0x00, 0x30]);
        let seek = compile_seek(TableSeekXml {
            name: "jump".into(),
            search_str: "FF @ @ @ @ AA +D4".into(),
            ..Default::default()
        }, 0).unwrap();
        assert_eq!(seek.jumps.len(), 1);
        assert!(seek.jumps[0].dword);
        assert_eq!(seek.jumps[0].offset, 4);
        let addr = address_from_hit(&bin, 4, &seek).unwrap();
        assert_eq!(addr, 0x30);
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
        assert!(
            r.located >= 1200,
            "expected 1200+ TableSeek hits on 12225074 (UP ~1500), got {} ({})",
            r.located, r.note
        );
        assert!(r.tables.iter().any(|t| t.name.contains("Volumetric") || t.name.contains("VE") || t.id.contains("volumetric")));
    }
}
