//! xdf.rs — XDF / TableData / TableSeek XML parser + real BIN byte extraction/patch
//!
//! Supports the practical definition formats present in reference/ (ArrayOfTableData
//! from 16263425.xml etc. and TableSeek from tableseek-p01-p59.xml). The main
//! OS-*.xdf is a binary TunerPro internal file and is not text XML.
//!
//! Provides commands for the frontend to obtain real TableDef metadata and to
//! perform authoritative extraction + patch of the loaded BIN bytes using the
//! *exact* P01 offsets (CAL_BASE + cal_rel_addr), big-endian packing, datatype,
//! and math formulas. Integrates with the existing checksum engine for safe edits.
//!
//! "Every byte mapped": the frontend builds an ownership map; these fns give the
//! precise offsets/sizes so the map is 100% accurate to the loaded BIN.

use serde::{Deserialize, Serialize};
use quick_xml::de::from_str;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableDef {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub description: String,
    pub rows: usize,
    pub cols: usize,
    /// Hex address. If `file_offset` is true, this is an index into the BIN file.
    pub addr: String,
    pub data_type: String, // UBYTE / UWORD / SWORD / SBYTE
    pub math: String,
    pub units: String,
    pub row_major: bool,
    #[serde(default = "default_true")]
    pub msb: bool,
    #[serde(default)]
    pub row_headers: Option<String>,
    #[serde(default)]
    pub col_headers: Option<String>,
    #[serde(default)]
    pub decimals: u8,
    #[serde(default)]
    pub file_offset: bool,
    /// Friendlier label from Universal Patcher ExtraTableName, when present.
    #[serde(default)]
    pub extra_name: Option<String>,
    #[serde(default)]
    pub extra_description: Option<String>,
    /// ECU family the address belongs to (catalog tables). Drives offset rules.
    #[serde(default)]
    pub family: Option<String>,
}

/// Lightweight result for a single table's physical values (already math-applied).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedTable {
    pub id: String,
    pub values: Vec<Vec<f64>>, // rows x cols physical
    pub axes_x: Vec<f64>,
    pub axes_y: Vec<f64>,
    pub note: Option<String>,
}

/// Patch request / response for two-way live editing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRequest {
    pub bin_bytes: Vec<u8>,
    pub table: TableDef,
    pub new_values: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchResult {
    pub patched_bytes: Vec<u8>,
    pub checksum_report: Option<crate::checksum::ChecksumReport>,
    pub message: String,
}

// --- XML shapes for deserialization (ArrayOfTableData style) ---
#[derive(Debug, Deserialize)]
struct ArrayOfTableData {
    #[serde(rename = "TableData", default)]
    table_data: Vec<TableDataXml>,
}

#[derive(Debug, Deserialize, Default)]
struct TableDataXml {
    #[serde(rename = "TableName", default)]
    name: String,
    #[serde(rename = "Address", default)]
    address: String,
    #[serde(rename = "Rows", default = "one")]
    rows: String,
    #[serde(rename = "Columns", default = "one")]
    cols: String,
    #[serde(rename = "Math", default)]
    math: String,
    #[serde(rename = "Units", default)]
    units: String,
    #[serde(rename = "TableDescription", default)]
    description: String,
    #[serde(rename = "DataType", default)]
    data_type: String,
    #[serde(rename = "Category", default)]
    category: String,
}

fn one() -> String { "1".into() }
fn default_true() -> bool { true }

/// Parse a TableData / TableSeek style XML (or snippet) into our TableDef list.
/// This is the "XDF XML parser" implementation (quick-xml).
/// For the binary .xdf we fall back to curated real tables (see JS side or future).
pub fn parse_table_definitions(xml: &str) -> Vec<TableDef> {
    let mut out = Vec::new();
    if let Ok(root) = from_str::<ArrayOfTableData>(xml) {
        for (i, t) in root.table_data.into_iter().enumerate() {
            let rows = t.rows.parse::<usize>().unwrap_or(1);
            let cols = t.cols.parse::<usize>().unwrap_or(1);
            // No address in the definition -> skip; never invent one.
            if t.address.trim().is_empty() { continue; }
            let addr = normalize_addr(&t.address);
            out.push(TableDef {
                id: sanitize_id(&t.name, i),
                name: if t.name.is_empty() { format!("Table_{}", i) } else { t.name },
                category: if t.category.is_empty() { None } else { Some(t.category) },
                description: t.description,
                rows,
                cols,
                addr,
                data_type: if t.data_type.is_empty() { "UWORD".into() } else { t.data_type },
                math: if t.math.is_empty() { "X".into() } else { t.math },
                units: t.units,
                row_major: true,
                msb: true,
                ..Default::default()
            });
        }
    }
    // TableSeek fallback (very common in reference)
    if out.is_empty() {
        // Extremely lightweight event scan for <TableSeek> ... <Name>..</Name> <RefAddress>.. etc.
        // (quick-xml de is strict; this keeps the demo working on tableseek XMLs without full schema)
        let mut cur_name = String::new();
        let mut cur_addr = String::new();
        let mut cur_rows = 1usize;
        let mut cur_cols = 1usize;
        let mut cur_math = "X".to_string();
        let mut cur_dtype = "UWORD".to_string();
        let mut cur_desc = String::new();
        let mut in_seek = false;

        for line in xml.lines() {
            let l = line.trim();
            if l.contains("<TableSeek") { in_seek = true; cur_name.clear(); cur_addr.clear(); cur_rows=1; cur_cols=1; cur_math="X".into(); cur_dtype="UWORD".into(); cur_desc.clear(); }
            if in_seek {
                if let Some(v) = extract_tag(l, "Name") { cur_name = v; }
                if let Some(v) = extract_tag(l, "RefAddress") { cur_addr = v; }
                if let Some(v) = extract_tag(l, "Rows") { cur_rows = v.parse().unwrap_or(1); }
                if let Some(v) = extract_tag(l, "Columns") { cur_cols = v.parse().unwrap_or(1); }
                if let Some(v) = extract_tag(l, "Math") { cur_math = v; }
                if let Some(v) = extract_tag(l, "DataType") { cur_dtype = v; }
                if let Some(v) = extract_tag(l, "Description") { cur_desc = v; }
                if l.contains("</TableSeek") {
                    if !cur_name.is_empty() {
                        let addr = if cur_addr.is_empty() { "0x00008000".into() } else { normalize_addr(&cur_addr) };
                        out.push(TableDef {
                            id: sanitize_id(&cur_name, out.len()),
                            name: cur_name.clone(),
                            category: None,
                            description: cur_desc.clone(),
                            rows: cur_rows,
                            cols: cur_cols,
                            addr,
                            data_type: cur_dtype.clone(),
                            math: cur_math.clone(),
                            units: "".into(),
                            row_major: true,
                            msb: true,
                            ..Default::default()
                        });
                    }
                    in_seek = false;
                }
            }
        }
    }
    out
}

fn extract_tag(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(s) = line.find(&open) {
        if let Some(e) = line.find(&close) {
            let start = s + open.len();
            if e > start { return Some(line[start..e].trim().to_string()); }
        }
    }
    None
}

fn normalize_addr(a: &str) -> String {
    let t = a.trim().trim_start_matches("0x").trim_start_matches("0X");
    if t.len() >= 4 { format!("0x{}", t) } else { format!("0x{:08X}", t.parse::<u32>().unwrap_or(0x8000)) }
}

fn sanitize_id(name: &str, i: usize) -> String {
    let mut s = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>();
    if s.is_empty() { s = format!("table_{}", i); }
    s
}

pub fn cell_width(data_type: &str) -> usize {
    let u = data_type.to_ascii_uppercase();
    if u.contains("32") || u.contains("LONG") || u.contains("DWORD") { 4 }
    else if u.contains("WORD") { 2 }
    else { 1 }
}

fn is_signed_type(data_type: &str) -> bool {
    let u = data_type.to_ascii_uppercase();
    u.starts_with('S') || u.starts_with("INT")
}

fn read_cell(bin: &[u8], off: usize, width: usize, signed: bool, msb: bool) -> f64 {
    if off + width > bin.len() { return 0.0; }
    let mut v: u32 = 0;
    if msb {
        for i in 0..width { v = (v << 8) | bin[off + i] as u32; }
    } else {
        for i in (0..width).rev() { v = (v << 8) | bin[off + i] as u32; }
    }
    if signed {
        match width {
            1 => v as i8 as f64,
            2 => v as i16 as f64,
            4 => v as i32 as f64,
            _ => v as f64,
        }
    } else {
        v as f64
    }
}

fn write_cell(bin: &mut [u8], off: usize, width: usize, raw: f64, msb: bool) {
    if off + width > bin.len() { return; }
    let max = match width {
        1 => 0xFFu32,
        2 => 0xFFFFu32,
        _ => 0xFFFF_FFFFu32,
    };
    let mut u = raw.round() as i64;
    if u < 0 {
        u += match width { 1 => 0x100, 2 => 0x10000, _ => 0x1_0000_0000 };
    }
    let mut v = u as u32;
    if v > max { v = max; }
    for i in 0..width {
        let shift = if msb { (width - 1 - i) * 8 } else { i * 8 };
        bin[off + i] = ((v >> shift) & 0xff) as u8;
    }
}

/// Extract physical values for one table from the BIN bytes using *exact* P01 addressing.
/// This is the core of "real byte extraction from loaded BINs using exact P01 offsets".
pub fn extract_table(bin: &[u8], def: &TableDef) -> ExtractedTable {
    let rows = def.rows.max(1);
    let cols = def.cols.max(1);
    let width = cell_width(&def.data_type);
    let signed = is_signed_type(&def.data_type);
    let mut values: Vec<Vec<f64>> = vec![vec![0.0; cols]; rows];
    let mut note = None;
    let off = match resolve_table_offset(def, bin.len()) {
        Ok(o) => o,
        Err(e) => return ExtractedTable { id: def.id.clone(), values, axes_x: vec![], axes_y: vec![], note: Some(e) },
    };
    let need = off + rows * cols * width;

    if need > bin.len() {
        note = Some("Address out of range for this BIN (full vs cal image?)".into());
        return ExtractedTable { id: def.id.clone(), values, axes_x: vec![], axes_y: vec![], note };
    }

    if let Err(e) = eval_math(&def.math, 0.0) {
        note = Some(format!("{}; raw values shown", e));
    }
    for r in 0..rows {
        for c in 0..cols {
            let raw = read_cell(bin, off + cell_index(def, r, c, rows, cols) * width, width, signed, def.msb);
            values[r][c] = apply_math(raw, &def.math);
        }
    }

    ExtractedTable {
        id: def.id.clone(),
        values,
        axes_x: vec![],
        axes_y: vec![],
        note,
    }
}

/// Patch new physical values back into a copy of the BIN at the exact address.
/// Returns the patched bytes (caller can then correct checksums).
/// Fail-closed: returns the BIN unchanged when the offset, size or math is not writable.
pub fn patch_table(mut bin: Vec<u8>, def: &TableDef, new_values: &[Vec<f64>]) -> Vec<u8> {
    let Ok(off) = resolve_table_offset(def, bin.len()) else { return bin };
    if linear_coeffs(&def.math).is_err() { return bin; }
    let width = cell_width(&def.data_type);
    let rows = new_values.len();
    let cols = new_values.first().map(|r| r.len()).unwrap_or(0);
    if new_values.iter().any(|r| r.len() != cols) { return bin; }
    if off + rows * cols * width > bin.len() { return bin; }
    for (r, row) in new_values.iter().enumerate() {
        for (c, phys) in row.iter().enumerate() {
            let Ok(raw) = inverse_math(*phys, &def.math) else { return bin };
            write_cell(&mut bin, off + cell_index(def, r, c, rows, cols) * width, width, raw, def.msb);
        }
    }
    bin
}

pub fn parse_addr(a: &str) -> usize {
    let t = a.trim().trim_start_matches("0x").trim_start_matches("0X");
    usize::from_str_radix(t, 16).unwrap_or(0)
}

/// File offset for a table address. All supported definitions (TableSeek, XDF,
/// P01 and catalog entries) use file offsets. There is no "+0x20000" base: for
/// P01 0x20000 is the start of the OS segment (reference/P01-P59.xml), not cal.
/// Honda catalog addresses are flagged "starting hints only" in
/// reference/ecu_database/honda_keihin.json with no base defined anywhere in the
/// repo, so they are refused rather than guessed.
pub fn resolve_table_offset(def: &TableDef, bin_len: usize) -> Result<usize, String> {
    let fam = def.family.as_deref().unwrap_or("").to_ascii_uppercase();
    if fam.contains("HONDA") || fam.contains("KEIHIN") {
        return Err(format!("Table '{}': Honda map addresses are unverified hints with no authoritative offset base. Refused.", def.name));
    }
    let addr = parse_addr_strict(&def.addr).ok_or_else(|| format!("Table '{}': bad address '{}'", def.name, def.addr))?;
    if addr >= bin_len {
        return Err(format!("Table '{}': address 0x{:X} beyond {}-byte BIN", def.name, addr, bin_len));
    }
    Ok(addr)
}

fn parse_addr_strict(a: &str) -> Option<usize> {
    let t = a.trim().trim_start_matches("0x").trim_start_matches("0X");
    usize::from_str_radix(t, 16).ok()
}

// ---------- Scaling math ----------
// XDF / TableSeek / A2L-style equations in X: numbers, X, + - * /, parentheses, unary minus.
// Patching needs the inverse, so only linear equations (a*X + b, a != 0) can be written back;
// anything else is extract-only and patch is refused (never write physical values as raw).

struct MathParser<'a> { s: &'a [u8], i: usize, x: f64 }

impl<'a> MathParser<'a> {
    fn ws(&mut self) { while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() { self.i += 1; } }
    fn peek(&mut self) -> Option<u8> { self.ws(); self.s.get(self.i).copied() }
    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        while let Some(c) = self.peek() {
            match c { b'+' => { self.i += 1; v += self.term()?; } b'-' => { self.i += 1; v -= self.term()?; } _ => break }
        }
        Ok(v)
    }
    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.factor()?;
        while let Some(c) = self.peek() {
            match c {
                b'*' => { self.i += 1; v *= self.factor()?; }
                b'/' => { self.i += 1; let d = self.factor()?; if d == 0.0 { return Err("division by zero".into()); } v /= d; }
                _ => break,
            }
        }
        Ok(v)
    }
    fn factor(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(b'-') => { self.i += 1; Ok(-self.factor()?) }
            Some(b'+') => { self.i += 1; self.factor() }
            Some(b'(') => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') { return Err("missing ')'".into()); }
                self.i += 1;
                Ok(v)
            }
            Some(b'X') | Some(b'x') => { self.i += 1; Ok(self.x) }
            Some(c) if c.is_ascii_digit() || c == b'.' => {
                let st = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') { self.i += 1; }
                if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
                    let save = self.i;
                    self.i += 1;
                    if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') { self.i += 1; }
                    let ds = self.i;
                    while self.i < self.s.len() && self.s[self.i].is_ascii_digit() { self.i += 1; }
                    if ds == self.i { self.i = save; }
                }
                std::str::from_utf8(&self.s[st..self.i]).ok().and_then(|t| t.parse::<f64>().ok())
                    .ok_or_else(|| "bad number".to_string())
            }
            Some(c) => Err(format!("unsupported token '{}'", c as char)),
            None => Err("unexpected end".into()),
        }
    }
}

/// Evaluate a scaling equation at `x`. Errors on anything outside the supported grammar.
pub fn eval_math(expr: &str, x: f64) -> Result<f64, String> {
    let e = expr.trim();
    if e.is_empty() { return Ok(x); }
    let mut p = MathParser { s: e.as_bytes(), i: 0, x };
    let v = p.expr().map_err(|m| format!("math '{}': {}", e, m))?;
    if p.peek().is_some() { return Err(format!("math '{}': trailing input", e)); }
    if !v.is_finite() { return Err(format!("math '{}': not finite", e)); }
    Ok(v)
}

/// (a, b) for a linear equation a*X + b with a != 0, else an error (patch refused).
pub fn linear_coeffs(expr: &str) -> Result<(f64, f64), String> {
    let b = eval_math(expr, 0.0)?;
    // Wide span keeps the slope accurate when the offset is large.
    let a = (eval_math(expr, 65536.0)? - b) / 65536.0;
    if a == 0.0 { return Err(format!("math '{}' is constant; cannot invert", expr.trim())); }
    for t in [2.0, 7.0, 255.0, 1000.0, 65535.0, -3.0] {
        let f = eval_math(expr, t)?;
        let lin = a * t + b;
        if (f - lin).abs() > 1e-9 * f.abs().max(lin.abs()).max(1.0) {
            return Err(format!("math '{}' is not linear in X; extract-only, patch refused", expr.trim()));
        }
    }
    Ok((a, b))
}

fn apply_math(raw: f64, expr: &str) -> f64 {
    eval_math(expr, raw).unwrap_or(raw)
}

fn inverse_math(phys: f64, expr: &str) -> Result<f64, String> {
    let (a, b) = linear_coeffs(expr)?;
    Ok((phys - b) / a)
}

/// Byte offset of cell (r, c) relative to the table start.
fn cell_index(def: &TableDef, r: usize, c: usize, rows: usize, cols: usize) -> usize {
    if def.row_major || rows == 1 || cols == 1 { r * cols + c } else { c * rows + r }
}


// ---------- TunerPro XDF (text XML, v1.x) ----------
// Element/attribute meanings follow TunerPro's XDF editor help
// (tunerpro.net/WebHelp/source/xdftableeditor.htm: Signed, LSB First) and RomRaider's
// XDF importer (github.com/RomRaider/RomRaider @dafe0c3,
// src/main/java/com/romraider/xml/ConversionLayer/XDFConversionLayer.java):
//   mmedtypeflags 0x01 signed, 0x02 LSB first, 0x04 column-major (RomRaider "swapxy"),
//   0x10000 IEEE float; CATEGORYMEM category is 1-based into header CATEGORY index;
//   BASEOFFSET offset is subtracted when subtract != 0.
// Unsupported layouts (float, strides, missing address) are skipped and reported, never guessed.

#[derive(Default, Clone)]
struct XdfAxis { addr: Option<String>, bits: Option<u32>, rows: Option<usize>, cols: Option<usize>, flags: Option<u32>,
    major: Option<u32>, minor: Option<u32>, units: String, math: Option<String>, decimals: Option<u8>,
    index_count: Option<usize>, labels: Vec<String> }

#[derive(Default)]
struct XdfItem { constant: bool, title: String, description: String, cats: Vec<usize>, axes: std::collections::HashMap<String, XdfAxis>,
    own: XdfAxis }

fn parse_num(v: &str) -> Option<i64> {
    let t = v.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) { i64::from_str_radix(h, 16).ok() } else { t.parse::<i64>().ok() }
}

/// Parsed TunerPro XDF: supported tables plus "title: reason" for each skipped item.
pub struct XdfParse { pub tables: Vec<TableDef>, pub skipped: Vec<String> }

pub fn parse_tunerpro_xdf(xml: &str) -> Result<XdfParse, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut rd = Reader::from_str(xml);
    rd.trim_text(true);
    let mut base: i64 = 0;
    let mut def_bits: u32 = 8;
    let mut def_signed = false;
    let mut def_lsb = false;
    let mut categories: std::collections::HashMap<usize, String> = Default::default();
    let mut items: Vec<XdfItem> = Vec::new();
    let mut cur: Option<XdfItem> = None;
    let mut axis_id: Option<String> = None;
    let mut text_tag = String::new();
    let attr = |e: &quick_xml::events::BytesStart, k: &str| -> Option<String> {
        e.attributes().flatten().find(|a| a.key.as_ref().eq_ignore_ascii_case(k.as_bytes()))
            .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
    };
    loop {
        let ev = rd.read_event().map_err(|e| format!("XDF XML error at byte {}: {}", rd.buffer_position(), e))?;
        match ev {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(ev, Event::Empty(_));
                let name = String::from_utf8_lossy(e.name().as_ref()).to_ascii_uppercase();
                match name.as_str() {
                    "BASEOFFSET" => {
                        let off = attr(e, "offset").and_then(|v| parse_num(&v)).unwrap_or(0);
                        let sub = attr(e, "subtract").map(|v| v.trim() != "0").unwrap_or(false);
                        base = if sub { -off } else { off };
                    }
                    "DEFAULTS" => {
                        def_bits = attr(e, "datasizeinbits").and_then(|v| parse_num(&v)).unwrap_or(8) as u32;
                        def_signed = attr(e, "signed").map(|v| v.trim() != "0").unwrap_or(false);
                        def_lsb = attr(e, "lsbfirst").map(|v| v.trim() != "0").unwrap_or(false);
                    }
                    "CATEGORY" if cur.is_none() => {
                        if let (Some(i), Some(n)) = (attr(e, "index").and_then(|v| parse_num(&v)), attr(e, "name")) {
                            categories.insert(i as usize, n);
                        }
                    }
                    "XDFTABLE" | "XDFCONSTANT" if !is_empty => {
                        cur = Some(XdfItem { constant: name == "XDFCONSTANT", ..Default::default() });
                    }
                    "XDFAXIS" if cur.is_some() && !is_empty => { axis_id = attr(e, "id").map(|v| v.to_ascii_lowercase()); }
                    "CATEGORYMEM" => {
                        if let (Some(it), Some(c)) = (cur.as_mut(), attr(e, "category").and_then(|v| parse_num(&v))) {
                            if c >= 1 { it.cats.push(c as usize - 1); }
                        }
                    }
                    "EMBEDDEDDATA" | "MATH" | "LABEL" => {
                        if let Some(it) = cur.as_mut() {
                            let ax = match axis_id.as_deref() {
                                Some(id) => it.axes.entry(id.to_string()).or_default(),
                                None => &mut it.own,
                            };
                            match name.as_str() {
                                "EMBEDDEDDATA" => {
                                    ax.addr = attr(e, "mmedaddress");
                                    ax.bits = attr(e, "mmedelementsizebits").and_then(|v| parse_num(&v)).map(|v| v as u32);
                                    ax.rows = attr(e, "mmedrowcount").and_then(|v| parse_num(&v)).map(|v| v as usize);
                                    ax.cols = attr(e, "mmedcolcount").and_then(|v| parse_num(&v)).map(|v| v as usize);
                                    ax.flags = attr(e, "mmedtypeflags").and_then(|v| parse_num(&v)).map(|v| v as u32);
                                    ax.major = attr(e, "mmedmajorstridebits").and_then(|v| parse_num(&v)).map(|v| v as u32);
                                    ax.minor = attr(e, "mmedminorstridebits").and_then(|v| parse_num(&v)).map(|v| v as u32);
                                }
                                "MATH" => { ax.math = attr(e, "equation"); }
                                _ => { if let Some(v) = attr(e, "value") { ax.labels.push(v); } }
                            }
                        }
                    }
                    _ => {}
                }
                if !is_empty { text_tag = name; }
            }
            Event::Text(t) => {
                let v = t.unescape().map(|c| c.into_owned()).unwrap_or_default();
                if let Some(it) = cur.as_mut() {
                    let ax = match axis_id.as_deref() {
                        Some(id) => Some(it.axes.entry(id.to_string()).or_default()),
                        None => None,
                    };
                    match (text_tag.as_str(), ax) {
                        ("TITLE", None) => it.title = v,
                        ("DESCRIPTION", None) => it.description = v,
                        ("UNITS", Some(a)) => a.units = v,
                        ("UNITS", None) => it.own.units = v,
                        ("DECIMALPL", Some(a)) => a.decimals = v.trim().parse().ok(),
                        ("DECIMALPL", None) => it.own.decimals = v.trim().parse().ok(),
                        ("INDEXCOUNT", Some(a)) => a.index_count = v.trim().parse().ok(),
                        _ => {}
                    }
                }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_ascii_uppercase();
                match name.as_str() {
                    "XDFAXIS" => axis_id = None,
                    "XDFTABLE" | "XDFCONSTANT" => { if let Some(it) = cur.take() { items.push(it); } }
                    _ => {}
                }
                text_tag.clear();
            }
            _ => {}
        }
    }
    let mut tables = Vec::new();
    let mut skipped = Vec::new();
    for (i, it) in items.into_iter().enumerate() {
        let title = if it.title.trim().is_empty() { format!("XDF item {}", i) } else { it.title.trim().to_string() };
        let z = if it.constant { it.own.clone() } else { it.axes.get("z").cloned().unwrap_or_default() };
        let skip = |why: &str| format!("{}: {}", title, why);
        let Some(addr_raw) = z.addr.as_deref().and_then(parse_num) else { skipped.push(skip("no data address")); continue };
        let flags = z.flags.unwrap_or(0);
        if flags & 0x10000 != 0 { skipped.push(skip("floating-point cells are not supported")); continue; }
        let bits = z.bits.unwrap_or(def_bits);
        let dtype = match (bits, flags & 0x01 != 0 || (z.flags.is_none() && def_signed)) {
            (8, false) => "UBYTE", (8, true) => "SBYTE", (16, false) => "UWORD", (16, true) => "SWORD",
            (32, false) => "ULONG", (32, true) => "SLONG",
            _ => { skipped.push(skip(&format!("{}-bit cells are not supported", bits))); continue; }
        };
        let (rows, cols) = if it.constant { (1, 1) } else { (z.rows.unwrap_or(1).max(1), z.cols.unwrap_or(1).max(1)) };
        let major = z.major.unwrap_or(0);
        if (major != 0 && major != bits && rows * cols > 1) || z.minor.unwrap_or(0) != 0 {
            skipped.push(skip("non-contiguous stride layout is not supported")); continue;
        }
        let addr = addr_raw + base;
        if addr < 0 { skipped.push(skip("address below base offset")); continue; }
        let math = z.math.clone().unwrap_or_else(|| "X".into());
        if let Err(e) = eval_math(&math, 0.0) { skipped.push(skip(&e)); continue; }
        let lsb = if z.flags.is_some() { flags & 0x02 != 0 } else { def_lsb };
        let column_major = flags & 0x04 != 0;
        let headers = |id: &str, n: usize| -> Option<String> {
            it.axes.get(id).filter(|a| a.labels.len() == n && n > 1).map(|a| a.labels.join(","))
        };
        let category = it.cats.first().and_then(|c| categories.get(c).cloned());
        tables.push(TableDef {
            id: sanitize_id(&title, i),
            name: title.clone(),
            category,
            description: it.description.trim().to_string(),
            rows,
            cols,
            addr: format!("0x{:X}", addr),
            data_type: dtype.into(),
            math,
            units: z.units.clone(),
            row_major: !column_major,
            msb: !lsb,
            row_headers: headers("y", rows),
            col_headers: headers("x", cols),
            decimals: z.decimals.unwrap_or(2),
            file_offset: true,
            ..Default::default()
        });
    }
    Ok(XdfParse { tables, skipped })
}

/// Parse report for the UI: tables plus the items that were skipped and why.
#[tauri::command]
pub fn parse_xdf_report(xml: String) -> Result<String, String> {
    check_xdf_text(&xml)?;
    if !xml.to_ascii_uppercase().contains("<XDFFORMAT") {
        let t = parse_table_definitions(&xml);
        return Ok(serde_json::json!({"format": "tabledata/tableseek", "tables": t.len(), "skipped": []}).to_string());
    }
    let r = parse_tunerpro_xdf(&xml)?;
    Ok(serde_json::json!({"format": "tunerpro-xdf", "tables": r.tables.len(), "skipped": r.skipped}).to_string())
}

// Tauri command wrappers (registered in lib.rs)
#[tauri::command]
pub fn parse_xdf_definitions(xml: String) -> Result<Vec<TableDef>, String> {
    check_xdf_text(&xml)?;
    if xml.to_ascii_uppercase().contains("<XDFFORMAT") {
        let r = parse_tunerpro_xdf(&xml)?;
        if r.tables.is_empty() {
            return Err(format!("TunerPro XDF parsed but no supported tables ({} skipped: {}).", r.skipped.len(), r.skipped.iter().take(5).cloned().collect::<Vec<_>>().join("; ")));
        }
        return Ok(r.tables);
    }
    let t = parse_table_definitions(&xml);
    if t.is_empty() { return Err("XDF parsed but contains no <XDFTABLE>/<XDFCONSTANT> definitions.".into()); }
    Ok(t)
}

/// Clear error for TunerPro binary/encrypted XDFs (e.g. reference/OS-12225074-512Kb-2001-2004.xdf).
pub fn check_xdf_text(xml: &str) -> Result<(), String> {
    let bad = xml.chars().take(4096).filter(|c| *c == '\u{FFFD}' || (c.is_control() && !c.is_whitespace())).count();
    if !xml.contains("<XDF") || bad > 8 {
        return Err("This XDF is not text XML (TunerPro binary or encrypted XDF). Re-save it from TunerPro as an uncompressed/unencrypted XDF, or use an A2L / definition pack.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn extract_table_from_bin(bin_bytes: Vec<u8>, table: TableDef) -> Result<ExtractedTable, String> {
    Ok(extract_table(&bin_bytes, &table))
}

#[tauri::command]
pub fn patch_table_into_bin(req: PatchRequest) -> Result<PatchResult, String> {
    let off = resolve_table_offset(&req.table, req.bin_bytes.len()).map_err(|e| format!("Refuse patch: {}", e))?;
    linear_coeffs(&req.table.math).map_err(|e| format!("Refuse patch: {}", e))?;
    let esz = cell_width(&req.table.data_type);
    let rows = req.new_values.len();
    let cols = req.new_values.first().map(|r| r.len()).unwrap_or(0);
    if req.new_values.iter().any(|r| r.len() != cols) {
        return Err("Refuse patch: ragged value grid".into());
    }
    if rows * cols != req.table.rows.max(1) * req.table.cols.max(1) {
        return Err(format!("Refuse patch: {}x{} values for a {}x{} table", rows, cols, req.table.rows, req.table.cols));
    }
    let need = off + rows * cols * esz;
    if need > req.bin_bytes.len() {
        return Err(format!(
            "Refuse patch: table '{}' needs offset 0x{:X}..0x{:X} but BIN is only {} bytes (addr {}).",
            req.table.name, off, need, req.bin_bytes.len(), req.table.addr
        ));
    }
    let patched = patch_table(req.bin_bytes, &req.table, &req.new_values);
    Ok(PatchResult {
        patched_bytes: patched,
        checksum_report: None,
        message: format!("Patched table {} at {} ({}x{})", req.table.name, req.table.addr, rows, cols),
    })
}
#[derive(Debug, Clone, Serialize)]
pub struct CellDelta {
    pub row: usize,
    pub col: usize,
    pub stock: f64,
    pub tuned: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDeltaReport {
    pub name: String,
    pub cells: usize,
    pub changed: usize,
    pub max_abs: f64,
    pub samples: Vec<CellDelta>,
    pub note: String,
}

/// Compare one map between a stock image and the working image. Does not write.
pub fn table_delta(stock: &[u8], tuned: &[u8], table: &TableDef) -> Result<TableDeltaReport, String> {
    if stock.len() != tuned.len() {
        return Err(format!("size mismatch: stock {} tuned {}", stock.len(), tuned.len()));
    }
    let a = extract_table(stock, table);
    let b = extract_table(tuned, table);
    if a.values.len() != b.values.len() {
        return Err("row mismatch after extract".into());
    }
    let mut samples = Vec::new();
    let mut changed = 0usize;
    let mut cells = 0usize;
    let mut max_abs = 0.0f64;
    for (r, (ra, rb)) in a.values.iter().zip(b.values.iter()).enumerate() {
        if ra.len() != rb.len() { return Err("column mismatch after extract".into()); }
        for (c, (va, vb)) in ra.iter().zip(rb.iter()).enumerate() {
            cells += 1;
            let delta = vb - va;
            if delta.abs() > 1e-9 {
                changed += 1;
                max_abs = max_abs.max(delta.abs());
                if samples.len() < 48 {
                    samples.push(CellDelta { row: r, col: c, stock: *va, tuned: *vb, delta });
                }
            }
        }
    }
    Ok(TableDeltaReport {
        name: table.name.clone(),
        cells,
        changed,
        max_abs,
        samples,
        note: "Cell delta is offline. It does not enable a write path.".into(),
    })
}

#[tauri::command]
pub fn table_delta_cmd(stock: Vec<u8>, tuned: Vec<u8>, table: TableDef) -> Result<String, String> {
    let report = table_delta(&stock, &tuned, &table)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

#[cfg(test)]
mod delta_tests {
    use super::*;

    #[test]
    fn delta_sees_one_word() {
        let mut stock = vec![0u8; 16];
        let mut tuned = stock.clone();
        tuned[0] = 0x00; tuned[1] = 0x0A;
        stock[0] = 0x00; stock[1] = 0x05;
        let table = TableDef {
            id: "t".into(),
            name: "scalar".into(),
            rows: 1,
            cols: 1,
            addr: "0".into(),
            data_type: "UWORD".into(),
            math: "X".into(),
            file_offset: true,
            msb: true,
            ..Default::default()
        };
        let r = table_delta(&stock, &tuned, &table).unwrap();
        assert_eq!(r.changed, 1);
        assert!((r.samples[0].delta - 5.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod offset_tests {
    use super::*;
    fn def(addr: &str, fam: Option<&str>) -> TableDef {
        TableDef { id: "t".into(), name: "t".into(), rows: 1, cols: 1, addr: addr.into(), data_type: "UBYTE".into(), math: "X".into(), msb: true, family: fam.map(Into::into), ..Default::default() }
    }
    #[test]
    fn p01_and_xdf_addresses_are_file_offsets_no_0x20000_base() {
        let mut bin = vec![0u8; 0x80000];
        bin[0x8021] = 24; bin[0x28021] = 99;
        for f in [None, Some("P01_0411")] {
            let d = def("0x8021", f);
            assert_eq!(resolve_table_offset(&d, bin.len()).unwrap(), 0x8021);
            assert_eq!(extract_table(&bin, &d).values[0][0], 24.0);
        }
        // 1MB / 2MB catalog tables are file offsets too (was +0x20000).
        assert_eq!(resolve_table_offset(&def("0x1C000", Some("ME7_COMMON")), 0x100000).unwrap(), 0x1C000);
        assert_eq!(resolve_table_offset(&def("0x80000", Some("EDC16C41")), 0x200000).unwrap(), 0x80000);
    }
    #[test]
    fn honda_catalog_addresses_refused() {
        let bin = vec![0u8; 0x80000];
        let d = def("0x8000", Some("HONDA_KEIHIN"));
        assert!(resolve_table_offset(&d, bin.len()).unwrap_err().contains("Honda"));
        assert!(extract_table(&bin, &d).note.unwrap().contains("Honda"));
        assert_eq!(patch_table(bin.clone(), &d, &[vec![5.0]]), bin, "patch must not write");
    }
    #[test]
    fn out_of_range_and_bad_addr_refused() {
        assert!(resolve_table_offset(&def("0x90000", None), 0x80000).is_err());
        assert!(resolve_table_offset(&def("zz", None), 0x80000).is_err());
        let bin = vec![0u8; 16];
        assert_eq!(patch_table(bin.clone(), &def("0xF", None), &[vec![1.0, 2.0]]), bin, "overrun refused");
    }
    #[test]
    fn honda_catalog_tables_tagged_and_refused() {
        let t = crate::ecu_database::get_tables_for_bin_size(0x80000);
        assert!(!t.is_empty());
        let bin = vec![0u8; 0x80000];
        for d in &t {
            assert_eq!(d.family.as_deref(), Some("HONDA_KEIHIN"));
            assert!(extract_table(&bin, d).note.unwrap_or_default().contains("Honda"));
        }
    }
}

#[cfg(test)]
mod xdf_text_tests {
    use super::*;
    #[test]
    fn binary_xdf_gets_clear_error() {
        let b = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../reference/OS-12225074-512Kb-2001-2004.xdf")).unwrap();
        let e = parse_xdf_definitions(String::from_utf8_lossy(&b).into_owned()).unwrap_err();
        assert!(e.contains("not text XML"));
        assert!(parse_xdf_definitions("<XDFFORMAT></XDFFORMAT>".into()).is_err());
    }
}

#[cfg(test)]
mod tunerpro_xdf_tests {
    use super::*;

    // Synthetic text XDF written for these tests (no real calibration data).
    const XDF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Written by TunerPro Version 5.00 -->
<XDFFORMAT version="1.60">
  <XDFHEADER>
    <flags>0x1</flags>
    <deftitle>synthetic</deftitle>
    <description>test only</description>
    <BASEOFFSET offset="0x10" subtract="1" />
    <DEFAULTS datasizeinbits="8" sigdigits="2" outputtype="1" signed="0" lsbfirst="0" float="0" />
    <REGION type="0xFFFFFFFF" startaddress="0x0" size="0x100" regionflags="0x0" name="Binary" desc="" />
    <CATEGORY index="0x0" name="Fuel" />
    <CATEGORY index="0x1" name="Spark" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1" flags="0x0">
    <title>Main Spark</title>
    <description>row-major big-endian words</description>
    <CATEGORYMEM index="0" category="2" />
    <XDFAXIS id="x" uniqueid="0x0">
      <indexcount>3</indexcount>
      <LABEL index="0" value="800" /><LABEL index="1" value="1600" /><LABEL index="2" value="2400" />
      <MATH equation="X"><VAR id="X" /></MATH>
    </XDFAXIS>
    <XDFAXIS id="y" uniqueid="0x0">
      <indexcount>2</indexcount>
      <LABEL index="0" value="20" /><LABEL index="1" value="100" />
      <MATH equation="X"><VAR id="X" /></MATH>
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedtypeflags="0x00" mmedaddress="0x30" mmedelementsizebits="16" mmedrowcount="2" mmedcolcount="3" mmedmajorstridebits="16" mmedminorstridebits="0" />
      <units>deg</units>
      <decimalpl>1</decimalpl>
      <MATH equation="X*0.1"><VAR id="X" /></MATH>
    </XDFAXIS>
  </XDFTABLE>
  <XDFTABLE uniqueid="0x2" flags="0x0">
    <title>Col Major Signed LE</title>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedtypeflags="0x07" mmedaddress="0x50" mmedelementsizebits="16" mmedrowcount="2" mmedcolcount="2" mmedmajorstridebits="0" mmedminorstridebits="0" />
      <MATH equation="(X+40)/2"><VAR id="X" /></MATH>
    </XDFAXIS>
  </XDFTABLE>
  <XDFCONSTANT uniqueid="0x3">
    <title>Idle Target &amp; Offset</title>
    <CATEGORYMEM index="0" category="1" />
    <EMBEDDEDDATA mmedaddress="0x70" mmedelementsizebits="8" />
    <units>rpm</units>
    <MATH equation="X*12.5-100"><VAR id="X" /></MATH>
  </XDFCONSTANT>
  <XDFTABLE uniqueid="0x4"><title>Float Table</title>
    <XDFAXIS id="z"><EMBEDDEDDATA mmedtypeflags="0x10000" mmedaddress="0x80" mmedelementsizebits="32" mmedrowcount="1" mmedcolcount="1" /></XDFAXIS>
  </XDFTABLE>
  <XDFTABLE uniqueid="0x5"><title>No Address</title>
    <XDFAXIS id="z"><units>x</units></XDFAXIS>
  </XDFTABLE>
  <XDFTABLE uniqueid="0x6"><title>Strided</title>
    <XDFAXIS id="z"><EMBEDDEDDATA mmedaddress="0x90" mmedelementsizebits="8" mmedrowcount="2" mmedcolcount="2" mmedmajorstridebits="16" /></XDFAXIS>
  </XDFTABLE>
  <XDFCONSTANT uniqueid="0x7"><title>Nonlinear</title>
    <EMBEDDEDDATA mmedaddress="0xA0" mmedelementsizebits="8" />
    <MATH equation="X*X"><VAR id="X" /></MATH>
  </XDFCONSTANT>
</XDFFORMAT>"#;

    fn by_name<'a>(t: &'a [TableDef], n: &str) -> &'a TableDef {
        t.iter().find(|d| d.name == n).unwrap_or_else(|| panic!("{} missing", n))
    }

    #[test]
    fn math_evaluator() {
        assert_eq!(eval_math("X", 5.0).unwrap(), 5.0);
        assert_eq!(eval_math("", 5.0).unwrap(), 5.0);
        assert!((eval_math("X*0.1-40", 500.0).unwrap() - 10.0).abs() < 1e-9);
        assert!((eval_math("(X+40)/2", 10.0).unwrap() - 25.0).abs() < 1e-9);
        assert!((eval_math("-x/4 + 1.5e1", 8.0).unwrap() - 13.0).abs() < 1e-9);
        assert!(eval_math("X^2", 1.0).is_err());
        assert!(eval_math("X/0", 1.0).is_err());
        assert!(eval_math("sin(X)", 1.0).is_err());
        let (a, b) = linear_coeffs("X*0.1-40").unwrap();
        assert!((a - 0.1).abs() < 1e-12 && (b + 40.0).abs() < 1e-12, "{} {}", a, b);
        assert!(linear_coeffs("X*X").unwrap_err().contains("not linear"));
        assert!(linear_coeffs("5").is_err());
        assert!((inverse_math(25.0, "(X+40)/2").unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn parses_tables_constants_and_reports_skips() {
        let r = parse_tunerpro_xdf(XDF).unwrap();
        assert_eq!(r.tables.len(), 4, "{:?}", r.skipped);
        let s = by_name(&r.tables, "Main Spark");
        assert_eq!((s.rows, s.cols), (2, 3));
        assert_eq!(s.addr, "0x20", "0x30 minus BASEOFFSET 0x10 (subtract=1)");
        assert_eq!(s.data_type, "UWORD");
        assert!(s.msb && s.row_major);
        assert_eq!(s.category.as_deref(), Some("Spark"));
        assert_eq!(s.col_headers.as_deref(), Some("800,1600,2400"));
        assert_eq!(s.row_headers.as_deref(), Some("20,100"));
        assert_eq!(s.decimals, 1);
        let c = by_name(&r.tables, "Col Major Signed LE");
        assert_eq!(c.data_type, "SWORD");
        assert!(!c.msb && !c.row_major);
        let k = by_name(&r.tables, "Idle Target & Offset");
        assert_eq!((k.rows, k.cols, k.data_type.as_str(), k.units.as_str()), (1, 1, "UBYTE", "rpm"));
        assert_eq!(k.category.as_deref(), Some("Fuel"));
        let sk = r.skipped.join(" | ");
        assert!(sk.contains("Float Table: floating-point"), "{}", sk);
        assert!(sk.contains("No Address: no data address"), "{}", sk);
        assert!(sk.contains("Strided: non-contiguous"), "{}", sk);
        assert!(by_name(&r.tables, "Nonlinear").math == "X*X");
        let via_cmd = parse_xdf_definitions(XDF.to_string()).unwrap();
        assert_eq!(via_cmd.len(), 4);
        let rep: serde_json::Value = serde_json::from_str(&parse_xdf_report(XDF.to_string()).unwrap()).unwrap();
        assert_eq!(rep["format"], "tunerpro-xdf");
        assert_eq!(rep["skipped"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn extract_patch_round_trip_row_and_column_major() {
        let r = parse_tunerpro_xdf(XDF).unwrap();
        let mut bin = vec![0u8; 0x100];
        // Main Spark @0x20, BE words 10,20,30 / 40,50,60 -> x0.1
        for (i, v) in [10u16, 20, 30, 40, 50, 60].iter().enumerate() {
            bin[0x20 + 2 * i..0x22 + 2 * i].copy_from_slice(&v.to_be_bytes());
        }
        // Col-major LE signed @0x40: storage order (r0c0, r1c0, r0c1, r1c1) = -40, 0, 2, 4
        for (i, v) in [-40i16, 0, 2, 4].iter().enumerate() {
            bin[0x40 + 2 * i..0x42 + 2 * i].copy_from_slice(&v.to_le_bytes());
        }
        bin[0x60] = 10; // Idle: 10*12.5-100 = 25
        let spark = by_name(&r.tables, "Main Spark");
        let e = extract_table(&bin, spark);
        assert_eq!(e.values, vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
        let cm = by_name(&r.tables, "Col Major Signed LE");
        let e2 = extract_table(&bin, cm);
        assert_eq!(e2.values, vec![vec![0.0, 21.0], vec![20.0, 22.0]]);
        let idle = by_name(&r.tables, "Idle Target & Offset");
        assert_eq!(extract_table(&bin, idle).values, vec![vec![25.0]]);

        // Patch the column-major table and read the bytes back in storage order.
        let out = patch_table_into_bin(PatchRequest { bin_bytes: bin.clone(), table: cm.clone(), new_values: vec![vec![1.0, 3.0], vec![2.0, 4.0]] }).unwrap();
        let raw: Vec<i16> = (0..4).map(|i| i16::from_le_bytes([out.patched_bytes[0x40 + 2 * i], out.patched_bytes[0x41 + 2 * i]])).collect();
        assert_eq!(raw, vec![-38, -36, -34, -32], "column-major storage, inverse of (X+40)/2");
        assert_eq!(extract_table(&out.patched_bytes, cm).values, vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
        // Only those 8 bytes changed.
        let changed = bin.iter().zip(&out.patched_bytes).filter(|(a, b)| a != b).count();
        assert!(changed <= 8 && changed > 0);

        let out2 = patch_table_into_bin(PatchRequest { bin_bytes: bin.clone(), table: spark.clone(), new_values: vec![vec![1.5, 2.0, 3.0], vec![4.0, 5.0, 6.0]] }).unwrap();
        assert_eq!(&out2.patched_bytes[0x20..0x22], &15u16.to_be_bytes());
    }

    #[test]
    fn nonlinear_or_wrong_shape_patch_is_refused() {
        let r = parse_tunerpro_xdf(XDF).unwrap();
        let bin = vec![0u8; 0x100];
        let nl = by_name(&r.tables, "Nonlinear");
        let e = patch_table_into_bin(PatchRequest { bin_bytes: bin.clone(), table: nl.clone(), new_values: vec![vec![4.0]] }).unwrap_err();
        assert!(e.contains("not linear"), "{}", e);
        assert_eq!(patch_table(bin.clone(), nl, &[vec![4.0]]), bin, "library patch is fail-closed too");
        let spark = by_name(&r.tables, "Main Spark");
        assert!(patch_table_into_bin(PatchRequest { bin_bytes: bin.clone(), table: spark.clone(), new_values: vec![vec![1.0]] }).unwrap_err().contains("values for a 2x3"));
    }

    #[test]
    fn tabledata_without_address_is_skipped_not_invented() {
        let xml = r#"<ArrayOfTableData><TableData><TableName>A</TableName><Address></Address></TableData><TableData><TableName>B</TableName><Address>0x10</Address></TableData></ArrayOfTableData>"#;
        let t = parse_table_definitions(xml);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "B");
    }
}
