//! GM multi-family segment checksums: identify, verify, compute, and an
//! in-memory "fix" that only returns bytes for export.
//!
//! Safety: nothing in this module talks to an ECU or changes a write gate.
//! `flash.rs` is untouched. P01 keeps its own separately gated experimental
//! write path; every other family here is identify / verify only.
//!
//! Provenance (see docs/GM_CHECKSUMS.md): the segment layouts below are
//! functional facts (pointer-table offsets, checksum word offsets, algorithm
//! parameters) re-stated in Rust. They were cross-checked against an
//! independent re-implementation that passes 267/267 stored checksums on 33
//! stock images. The original public description of these layouts is the
//! Universal Patcher project (GPL-3.0); no config text, table or code from it
//! is copied into this repository.
//!
//! Model: for each checksum, data = concat(inclusive blocks) minus `exclude`
//! ranges minus the checksum field itself; v = method(data); optional one's /
//! two's complement; masked to the field width; optional byte swap; stored
//! big-endian (all GM families here are MSB-first).

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Method {
    /// CRC-16/ARC: poly 0x8005 reflected (0xA001), init 0, no xorout.
    Crc16Arc,
    /// CRC-32/ISO-HDLC (zlib).
    Crc32,
    ByteSum,
    /// Sum of 16-bit words (trailing odd byte ignored).
    WordSum16,
    /// Sum of 32-bit dwords (trailing partial dword ignored).
    DwordSum32,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Crc16Arc => "CRC-16/ARC",
            Method::Crc32 => "CRC-32",
            Method::ByteSum => "byte sum",
            Method::WordSum16 => "16-bit word sum",
            Method::DwordSum32 => "32-bit dword sum",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Complement {
    None,
    Ones,
    Twos,
}

/// One resolved checksum: where it is stored and what it covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CsDef {
    pub segment: String,
    /// "cs1" or "cs2".
    pub which: &'static str,
    pub method: Method,
    pub addr: usize,
    pub bytes: usize,
    pub complement: Complement,
    pub swap: bool,
    pub msb: bool,
    /// Inclusive byte ranges.
    pub blocks: Vec<(usize, usize)>,
    /// Inclusive ranges cut out of a block that fully contains them.
    pub exclude: Vec<(usize, usize)>,
}

// ── primitive algorithms ──────────────────────────────────────────────────

/// CRC-16/ARC. Same algorithm as `checksum::crc16_arc` (P01 CVN); delegated
/// there so there is one implementation.
pub fn crc16_arc(data: &[u8]) -> u16 {
    crate::checksum::crc16_arc(&[data])
}

pub fn crc32_ieee(data: &[u8]) -> u32 {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn width_mask(bytes: usize) -> u64 {
    if bytes >= 8 { u64::MAX } else { (1u64 << (8 * bytes)) - 1 }
}

fn read_be(buf: &[u8], at: usize, n: usize) -> Option<u64> {
    buf.get(at..at.checked_add(n)?).map(|s| s.iter().fold(0u64, |a, &b| (a << 8) | b as u64))
}

fn read_uint(buf: &[u8], at: usize, n: usize, msb: bool) -> Option<u64> {
    let s = buf.get(at..at.checked_add(n)?)?;
    Some(if msb {
        s.iter().fold(0u64, |a, &b| (a << 8) | b as u64)
    } else {
        s.iter().rev().fold(0u64, |a, &b| (a << 8) | b as u64)
    })
}

/// Bytes covered by `cs`, in order, without the checksum field and exclusions.
pub fn gather(buf: &[u8], cs: &CsDef) -> Result<Vec<u8>, String> {
    let mut blocks = Vec::new();
    for &(s0, e) in &cs.blocks {
        let mut s = s0;
        for &(xs, xe) in &cs.exclude {
            if s <= xs && xe <= e {
                if xs > s { blocks.push((s, xs - 1)); }
                s = xe + 1;
            }
        }
        blocks.push((s, e));
    }
    let (a, n) = (cs.addr, cs.bytes);
    let mut out = Vec::new();
    for (s0, e) in blocks {
        if e >= buf.len() {
            return Err(format!("{} {}: block end 0x{:X} is past the image end (0x{:X})", cs.segment, cs.which, e, buf.len()));
        }
        let mut s = s0;
        if s <= a && a <= e {
            if a == s {
                s += n;
            } else {
                out.extend_from_slice(&buf[s..a]);
                s = a + n;
            }
        }
        if s <= e { out.extend_from_slice(&buf[s..=e]); }
    }
    Ok(out)
}

/// Checksum value as it should be stored (already complemented / masked / swapped).
pub fn compute(buf: &[u8], cs: &CsDef) -> Result<u64, String> {
    let d = gather(buf, cs)?;
    let mut s: u64 = match cs.method {
        Method::Crc16Arc => crc16_arc(&d) as u64,
        Method::Crc32 => crc32_ieee(&d) as u64,
        Method::ByteSum => d.iter().fold(0u64, |a, &b| a.wrapping_add(b as u64)),
        Method::WordSum16 => d.chunks_exact(2).fold(0u64, |a, w| {
            let v = if cs.msb { u16::from_be_bytes([w[0], w[1]]) } else { u16::from_le_bytes([w[0], w[1]]) };
            a.wrapping_add(v as u64)
        }),
        Method::DwordSum32 => d.chunks_exact(4).fold(0u64, |a, w| {
            let b = [w[0], w[1], w[2], w[3]];
            let v = if cs.msb { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) };
            a.wrapping_add(v as u64)
        }),
    };
    s = match cs.complement {
        Complement::None => s,
        Complement::Ones => !s,
        Complement::Twos => s.wrapping_neg(),
    };
    s &= width_mask(cs.bytes);
    if cs.swap {
        let be = s.to_be_bytes();
        let field = &be[8 - cs.bytes..];
        s = field.iter().rev().fold(0u64, |a, &b| (a << 8) | b as u64);
    }
    Ok(s)
}

pub fn stored(buf: &[u8], cs: &CsDef) -> Result<u64, String> {
    read_uint(buf, cs.addr, cs.bytes, cs.msb)
        .ok_or_else(|| format!("{} {}: checksum field 0x{:X} is past the image end", cs.segment, cs.which, cs.addr))
}

fn write_stored(buf: &mut [u8], cs: &CsDef, v: u64) {
    for i in 0..cs.bytes {
        let shift = 8 * if cs.msb { cs.bytes - 1 - i } else { i };
        buf[cs.addr + i] = (v >> shift) as u8;
    }
}

// ── families and image layouts ────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Family {
    P01P59,
    P04V6,
    P08P10,
    Ls1P30,
    E38,
    E92,
    T43,
    DuramaxE54,
    DuramaxE60,
    VortecBlackbox9697,
}

pub const ALL_FAMILIES: [Family; 10] = [
    Family::P01P59, Family::P04V6, Family::P08P10, Family::Ls1P30, Family::E38,
    Family::E92, Family::T43, Family::DuramaxE54, Family::DuramaxE60, Family::VortecBlackbox9697,
];

impl Family {
    pub fn id(self) -> &'static str {
        match self {
            Family::P01P59 => "GM_P01_P59",
            Family::P04V6 => "GM_P04_V6",
            Family::P08P10 => "GM_P08_P10",
            Family::Ls1P30 => "GM_LS1_97_98",
            Family::E38 => "GM_E38",
            Family::E92 => "GM_E92",
            Family::T43 => "GM_T43",
            Family::DuramaxE54 => "GM_DURAMAX_E54",
            Family::DuramaxE60 => "GM_DURAMAX_E60",
            Family::VortecBlackbox9697 => "GM_VORTEC_96_97_BLACKBOX",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Family::P01P59 => "GM P01 (LS1 99-02, 512KB)",
            Family::P04V6 => "GM P04 3800 V6 (512KB)",
            Family::P08P10 => "GM P08/P10 (Atlas L6, 512KB)",
            Family::Ls1P30 => "GM LS1 97-98 / P30 (512KB)",
            Family::E38 => "GM E38 (2MB)",
            Family::E92 => "GM E92 / E92A (4MB)",
            Family::T43 => "GM T43 TCM (2MB)",
            Family::DuramaxE54 => "GM Duramax E54 LB7 (512KB)",
            Family::DuramaxE60 => "GM Duramax E60 LLY (1MB)",
            Family::VortecBlackbox9697 => "GM 96-97 Vortec blackbox (256KB)",
        }
    }
    /// Size of the full image the layout addresses refer to.
    pub fn full_size(self) -> usize {
        match self {
            Family::VortecBlackbox9697 => 0x40000,
            Family::P01P59 | Family::P04V6 | Family::P08P10 | Family::Ls1P30 | Family::DuramaxE54 => 0x80000,
            Family::DuramaxE60 => 0x100000,
            Family::E38 | Family::T43 => 0x200000,
            Family::E92 => 0x400000,
        }
    }
    pub fn from_id(s: &str) -> Option<Family> {
        let k = s.trim().to_ascii_uppercase().replace(['-', ' ', '/'], "_");
        ALL_FAMILIES.iter().copied().find(|f| f.id() == k || f.id().trim_start_matches("GM_") == k)
    }
    /// Short files (TunerCat cal+OS dumps, reads without the boot block) that
    /// can be rebuilt into this family's full layout.
    fn remaps(self) -> &'static [Remap] {
        match self {
            Family::P01P59 | Family::P08P10 => &[Remap::None, Remap::TunerCatP01],
            Family::DuramaxE54 => &[Remap::None, Remap::TunerCatE54],
            Family::DuramaxE60 => &[Remap::None, Remap::TunerCatE60],
            Family::E38 => &[Remap::None, Remap::MissingBoot { missing: 0x10000 }],
            Family::T43 => &[Remap::None, Remap::MissingBoot { missing: 0x30000 }],
            Family::E92 => &[Remap::None, Remap::MissingBoot { missing: 0x40000 }],
            _ => &[Remap::None],
        }
    }
}

/// How a file on disk maps onto the family's full image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Remap {
    None,
    /// 0x7C000-byte TunerCat P01/P08 file -> 512KB.
    TunerCatP01,
    /// 0x68000-byte TunerCat E54 file -> 512KB.
    TunerCatE54,
    /// 0xA8000-byte TunerCat E60 file -> 1MB.
    TunerCatE60,
    /// Read that starts after a boot block of `missing` bytes; padded with 0xFF.
    MissingBoot { missing: usize },
}

impl Remap {
    pub fn label(self) -> String {
        match self {
            Remap::None => "none".into(),
            Remap::TunerCatP01 => "TunerCat P01 0x7C000 -> 512KB".into(),
            Remap::TunerCatE54 => "TunerCat E54 0x68000 -> 512KB".into(),
            Remap::TunerCatE60 => "TunerCat E60 0xA8000 -> 1MB".into(),
            Remap::MissingBoot { missing } => format!("boot block 0x{:X} missing, padded 0xFF", missing),
        }
    }
    fn source_len(self, full: usize) -> usize {
        match self {
            Remap::None => full,
            Remap::TunerCatP01 => 0x7C000,
            Remap::TunerCatE54 => 0x68000,
            Remap::TunerCatE60 => 0xA8000,
            Remap::MissingBoot { missing } => full - missing,
        }
    }
    /// Rebuild the full image. Errors when the file size does not fit.
    pub fn apply(self, raw: &[u8], full: usize) -> Result<Vec<u8>, String> {
        if raw.len() != self.source_len(full) {
            return Err(format!("{} expects 0x{:X} bytes, file is 0x{:X}", self.label(), self.source_len(full), raw.len()));
        }
        let ff = |n: usize| vec![0xFFu8; n];
        let out = match self {
            Remap::None => raw.to_vec(),
            Remap::TunerCatP01 => [&raw[..0x4000], &ff(0x4000), &raw[0x64000..], &raw[0x4000..0x64000]].concat(),
            Remap::TunerCatE54 => [&ff(0x8000)[..], &raw[0x50000..], &raw[..0x50000], &ff(0x10000)].concat(),
            Remap::TunerCatE60 => [&ff(0x8000)[..], &raw[0x80000..], &ff(0x10000), &raw[..0x80000], &ff(0x40000)].concat(),
            Remap::MissingBoot { missing } => [&ff(missing)[..], raw].concat(),
        };
        debug_assert_eq!(out.len(), full);
        Ok(out)
    }
}

/// Start/end (inclusive) u32 big-endian pair from a segment pointer table.
fn pair(buf: &[u8], at: usize) -> Result<(usize, usize), String> {
    let s = read_be(buf, at, 4).ok_or_else(|| format!("pointer @0x{:X} past image end", at))? as usize;
    let e = read_be(buf, at + 4, 4).ok_or_else(|| format!("pointer @0x{:X} past image end", at + 4))? as usize;
    if s > e || e >= buf.len() {
        return Err(format!("pointer pair @0x{:X} = 0x{:X}-0x{:X} is not inside the 0x{:X}-byte image", at, s, e, buf.len()));
    }
    Ok((s, e))
}

fn pairs(buf: &[u8], at: usize, n: usize) -> Result<Vec<(usize, usize)>, String> {
    (0..n).map(|i| pair(buf, at + 8 * i)).collect()
}

fn wordsum2(segment: &str, addr: usize, blocks: Vec<(usize, usize)>) -> CsDef {
    CsDef { segment: segment.into(), which: "cs1", method: Method::WordSum16, addr, bytes: 2, complement: Complement::Twos, swap: false, msb: true, blocks, exclude: vec![] }
}

/// E38 / E92 / T43 style segment header: CRC-16 (byte-swapped) at `crc_off`
/// over `crc_blocks` pairs, two's-complement word sum at `sum_off` over
/// `sum_blocks` pairs; both offsets relative to the segment start.
fn crc_and_sum(buf: &[u8], name: &str, seg_start: usize, crc_off: usize, crc_tbl: usize, crc_n: usize, sum_off: usize, sum_tbl: usize, sum_n: usize) -> Result<Vec<CsDef>, String> {
    Ok(vec![
        CsDef { segment: name.into(), which: "cs1", method: Method::Crc16Arc, addr: seg_start + crc_off, bytes: 2, complement: Complement::None, swap: true, msb: true, blocks: pairs(buf, crc_tbl, crc_n)?, exclude: vec![] },
        CsDef { segment: name.into(), which: "cs2", method: Method::WordSum16, addr: seg_start + sum_off, bytes: 2, complement: Complement::Twos, swap: false, msb: true, blocks: pairs(buf, sum_tbl, sum_n)?, exclude: vec![] },
    ])
}

/// Simple pointer-table cal segments: checksum word at the segment start,
/// covering the whole segment (the word itself is skipped).
fn simple_segments(buf: &[u8], table: &[(&str, usize)]) -> Result<Vec<CsDef>, String> {
    table.iter().map(|&(name, at)| {
        let (s, e) = pair(buf, at)?;
        Ok(wordsum2(name, s, vec![(s, e)]))
    }).collect()
}

/// Resolved checksums for `buf` (already in the family's full layout), the
/// OS id where the family stores one, and notes on segments not present.
#[derive(Clone, Debug, Serialize)]
pub struct Layout {
    pub checksums: Vec<CsDef>,
    pub os_id: Option<u32>,
    pub notes: Vec<String>,
}

fn u32_at(buf: &[u8], at: usize) -> Option<u32> {
    read_be(buf, at, 4).map(|v| v as u32)
}

/// P04 (GM V6): the OS id sits just before an 0xA55A word near the image end
/// (that id is excluded from the OS sum); the checksum address is taken from
/// the 68k code that loads it: a unique `move.l (abs).l` of the last flash
/// dword, followed within 16..32 bytes by the same opcode with the address.
fn p04_cs_addr(buf: &[u8]) -> Result<(usize, usize), String> {
    let n = buf.len();
    let mut pn = None;
    for i in 2..20 {
        if read_be(buf, n - i, 2) == Some(0xA55A) { pn = Some(n - (i + 4)); }
    }
    let pn = pn.ok_or("P04: OS id marker 0xA55A not found near image end")?;
    let pats: &[[u8; 6]] = match n {
        0x40000 => &[[0x20, 0x39, 0, 3, 0xFF, 0xFA]],
        0x80000 => &[[0x26, 0x39, 0, 7, 0xFF, 0xFA], [0x20, 0x39, 0, 7, 0xFF, 0xFA], [0x20, 0x39, 0, 7, 0xFF, 0xF8], [0x26, 0x39, 0, 7, 0xFF, 0xF8]],
        0x100000 => &[[0x26, 0x39, 0, 0x0F, 0xFF, 0xFA]],
        _ => &[],
    };
    for p in pats {
        let hits: Vec<usize> = buf.windows(6).enumerate().filter(|(_, w)| *w == p).map(|(i, _)| i).take(2).collect();
        if hits.len() != 1 { continue; }
        let i = hits[0];
        for j in (i + 16)..(i + 32).min(n.saturating_sub(5)) {
            if buf[j] == p[0] && buf[j + 1] == p[1] {
                let a = u32_at(buf, j + 2).unwrap() as usize;
                if a + 4 > n { return Err(format!("P04: checksum address 0x{:X} outside image", a)); }
                return Ok((a, pn));
            }
        }
        break;
    }
    Err("P04: checksum address code pattern not found (unknown OS layout)".into())
}

pub fn resolve(family: Family, buf: &[u8]) -> Result<Layout, String> {
    if buf.len() != family.full_size() {
        return Err(format!("{} needs a 0x{:X}-byte image, got 0x{:X}", family.label(), family.full_size(), buf.len()));
    }
    let mut notes = vec![];
    let (checksums, os_id) = match family {
        Family::P01P59 => {
            // Reuse the existing P01 segment reader (checksum.rs) so there is
            // one source of truth for P01 CS1.
            let segs = crate::checksum::p01_segments(buf)?;
            let cs = segs.into_iter().map(|s| wordsum2(s.name, s.cs_addr, s.blocks)).collect();
            notes.push("CS2 (CRC-16 CVN) is not stored in the image; see the P01 CVN report.".into());
            (cs, u32_at(buf, 0x504))
        }
        Family::P08P10 => {
            let mut cs = vec![wordsum2("OS", 0x52A, vec![(0, 0x529), (0x52C, 0x3FFF), (0x20000, buf.len() - 5)])];
            cs.extend(simple_segments(buf, &[("Engine", 0x54E), ("Trans", 0x556), ("Speedo", 0x55E), ("System", 0x566)])?);
            (cs, u32_at(buf, 0x52E))
        }
        Family::P04V6 => {
            let (a, pn) = p04_cs_addr(buf)?;
            let cs = CsDef { segment: "OS".into(), which: "cs1", method: Method::WordSum16, addr: a, bytes: 4, complement: Complement::None, swap: false, msb: true,
                blocks: vec![(0, 0x3FFF), (0x8000, buf.len() - 1)], exclude: vec![(pn, pn + 3)] };
            (vec![cs], u32_at(buf, pn))
        }
        Family::Ls1P30 => {
            let cs = CsDef { segment: "OS".into(), which: "cs1", method: Method::WordSum16, addr: 0x6F294, bytes: 4, complement: Complement::None, swap: false, msb: true,
                blocks: vec![(0, 0x3FFF), (0x8000, 0x6F293), (0x6F298, 0x6F2B7), (0x6F2BC, buf.len() - 1)], exclude: vec![] };
            (vec![cs], u32_at(buf, 0x6F2B8))
        }
        Family::E38 => {
            let mut cs = vec![];
            match pair(buf, 0x414).and_then(|(s, _)| crc_and_sum(buf, "BootBlock", s, 0x40E, 0x41C, 3, 0x3F0, 0x434, 2)) {
                Ok(v) => cs.extend(v),
                Err(e) => notes.push(format!("BootBlock not checked ({})", e)),
            }
            let mut os_id = None;
            for (name, x) in [("OS", 0x10024), ("System", 0x10048), ("Fuel", 0x1006B), ("Speedo", 0x1008E), ("EngineDiag", 0x100B1), ("Engine", 0x100D4)] {
                let (s, _) = pair(buf, x)?;
                if name == "OS" { os_id = u32_at(buf, s + 5); }
                cs.extend(crc_and_sum(buf, name, s, 0x1E, x + 8, 2, 0, x + 24, 1)?);
            }
            (cs, os_id)
        }
        Family::E92 => {
            let mut cs = vec![];
            match pair(buf, 0xFDA5).and_then(|(s, _)| crc_and_sum(buf, "Boot Block", s, 0xFDA0, 0xFDAD, 3, 0xFD80, 0xFDC5, 2)) {
                Ok(v) => cs.extend(v),
                Err(e) => notes.push(format!("Boot Block not checked ({})", e)),
            }
            let (os, _) = pair(buf, 0xC0126)?;
            cs.extend(crc_and_sum(buf, "OS", os, 0x120, 0xC0136, 3, 0x100, 0xC014E, 2)?);
            for (name, x) in [("System", 0xC0162), ("Fuel", 0xC0186), ("Speedo", 0xC01AA), ("EngineDiag", 0xC01CE), ("Engine", 0xC01F2)] {
                let (s, _) = pair(buf, x)?;
                cs.extend(crc_and_sum(buf, name, s, 0x20, x + 8, 2, 0, x + 24, 1)?);
            }
            (cs, u32_at(buf, os + 0x105))
        }
        Family::T43 => {
            let mut cs = vec![];
            match pair(buf, 0x2A022).and_then(|(s, _)| crc_and_sum(buf, "BootBlock", s, 0xA01C, 0x2A02A, 3, 0xA000, 0x2A042, 2)) {
                Ok(v) => cs.extend(v),
                Err(e) => notes.push(format!("BootBlock not checked ({})", e)),
            }
            let (os, _) = pair(buf, 0x60022)?;
            cs.extend(crc_and_sum(buf, "OS", os, 0x3001C, 0x60032, 4, 0x30000, 0x60052, 3)?);
            for (name, x) in [("Trans", 0x6006E), ("Diag", 0x60091), ("System", 0x600B4)] {
                let (s, _) = pair(buf, x)?;
                cs.extend(crc_and_sum(buf, name, s, 0x1C, x + 8, 2, 0, x + 24, 1)?);
            }
            (cs, u32_at(buf, os + 0x30005))
        }
        Family::DuramaxE54 => {
            let (s, e) = pair(buf, 0x2000A)?;
            let mut cs = vec![wordsum2("OS", 0x20000, vec![(s, e)])];
            cs.extend(simple_segments(buf, &[("Engine", 0x20012), ("EngineDiag", 0x2001A), ("Fuel", 0x20022), ("System", 0x2002A), ("Speedo", 0x20032)])?);
            (cs, u32_at(buf, 0x20004))
        }
        Family::DuramaxE60 => {
            let (s, e) = pair(buf, 0x4000A)?;
            let mut cs = vec![wordsum2("OS", 0x40000, vec![(s, e)])];
            cs.extend(simple_segments(buf, &[("EngineOper", 0x40012), ("Engine", 0x4001A), ("EngineDiag", 0x40022), ("Fuel", 0x4002A), ("System", 0x40032), ("Speedo", 0x4003A)])?);
            (cs, u32_at(buf, 0x40004))
        }
        Family::VortecBlackbox9697 => {
            let (s, e) = pair(buf, 0x40C)?;
            let mut cs = vec![wordsum2("OS", 0x400, vec![(s, e)])];
            cs.extend(simple_segments(buf, &[("EngineCal", 0x414), ("System/Hvac", 0x41C), ("Ebc/System", 0x424), ("Speedo", 0x42C), ("vin", 0x434), ("TransCal", 0x43C)])?);
            (cs, u32_at(buf, 0x404))
        }
    };
    for c in &checksums {
        if c.addr + c.bytes > buf.len() {
            return Err(format!("{} {}: checksum field 0x{:X} outside image", c.segment, c.which, c.addr));
        }
    }
    Ok(Layout { checksums, os_id, notes })
}

// ── reports ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
pub struct CsResult {
    pub segment: String,
    pub which: &'static str,
    pub method: &'static str,
    pub addr: String,
    pub bytes: usize,
    pub stored: String,
    pub computed: String,
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FamilyReport {
    pub family: &'static str,
    pub label: &'static str,
    pub remap: String,
    pub image_size: usize,
    pub os_id: Option<u32>,
    pub passed: usize,
    pub total: usize,
    pub all_valid: bool,
    pub checksums: Vec<CsResult>,
    pub notes: Vec<String>,
    /// Always states that this module never writes to an ECU.
    pub write_support: &'static str,
}

pub const WRITE_NOTE_P01: &str = "Verify / export only. Flashing P01 stays behind the separate experimental write gate; this report does not enable it.";
pub const WRITE_NOTE_OTHER: &str = "Identify / verify only. No flash path exists for this family; corrected images are for export only.";

fn write_note(f: Family) -> &'static str {
    if f == Family::P01P59 { WRITE_NOTE_P01 } else { WRITE_NOTE_OTHER }
}

pub fn verify_layout(buf: &[u8], layout: &Layout) -> Vec<CsResult> {
    layout.checksums.iter().map(|c| {
        let st = stored(buf, c).ok();
        let co = compute(buf, c).ok();
        let w = 2 * c.bytes;
        let hex = |v: Option<u64>| v.map(|v| format!("0x{:0w$X}", v, w = w)).unwrap_or_else(|| "n/a".into());
        CsResult { segment: c.segment.clone(), which: c.which, method: c.method.label(), addr: format!("0x{:X}", c.addr), bytes: c.bytes,
            stored: hex(st), computed: hex(co), ok: st.is_some() && st == co }
    }).collect()
}

fn report(family: Family, remap: Remap, buf: &[u8], layout: &Layout) -> FamilyReport {
    let checksums = verify_layout(buf, layout);
    let passed = checksums.iter().filter(|c| c.ok).count();
    let total = checksums.len();
    FamilyReport { family: family.id(), label: family.label(), remap: remap.label(), image_size: buf.len(), os_id: layout.os_id,
        passed, total, all_valid: total > 0 && passed == total, checksums, notes: layout.notes.clone(), write_support: write_note(family) }
}

/// Verify `raw` as `family` (trying the family's known short-file layouts).
pub fn verify_family(family: Family, raw: &[u8]) -> Result<FamilyReport, String> {
    let mut errs = vec![];
    for &rm in family.remaps() {
        let Ok(buf) = rm.apply(raw, family.full_size()) else { continue };
        match resolve(family, &buf) {
            Ok(l) => return Ok(report(family, rm, &buf, &l)),
            Err(e) => errs.push(e),
        }
    }
    if errs.is_empty() {
        Err(format!("{}: no known layout for a 0x{:X}-byte file", family.label(), raw.len()))
    } else {
        Err(format!("{}: {}", family.label(), errs.join("; ")))
    }
}

/// Every family whose layout resolves on this file, best match first
/// (all checksums valid, then most passing).
pub fn identify(raw: &[u8]) -> Vec<FamilyReport> {
    let mut out: Vec<FamilyReport> = ALL_FAMILIES.iter().filter_map(|&f| verify_family(f, raw).ok()).collect();
    out.sort_by_key(|r| (std::cmp::Reverse(r.all_valid), std::cmp::Reverse(r.passed)));
    out
}

/// Recompute every stored checksum in memory. Returns the full-layout image
/// (rebuilt if the input was a short file) and the post-fix report. The bytes
/// are only for export: nothing here can reach an ECU.
pub fn fix_family(family: Family, raw: &[u8]) -> Result<(Vec<u8>, FamilyReport, usize), String> {
    for &rm in family.remaps() {
        let Ok(mut buf) = rm.apply(raw, family.full_size()) else { continue };
        let Ok(layout) = resolve(family, &buf) else { continue };
        let mut changed = 0usize;
        // One checksum's blocks can cover another's field: repeat until stable.
        for _ in 0..4 {
            let mut any = false;
            for c in &layout.checksums {
                let v = compute(&buf, c)?;
                if stored(&buf, c)? != v {
                    write_stored(&mut buf, c, v);
                    any = true;
                    changed += 1;
                }
            }
            if !any { break; }
        }
        let rep = report(family, rm, &buf, &layout);
        if !rep.all_valid {
            return Err(format!("{}: post-fix verify failed ({}/{})", family.label(), rep.passed, rep.total));
        }
        return Ok((buf, rep, changed));
    }
    Err(format!("{}: layout does not resolve on this file; nothing corrected", family.label()))
}

// ── Tauri commands (offline; no ECU) ──────────────────────────────────────

#[tauri::command]
pub fn gm_checksum_families() -> Result<String, String> {
    let v: Vec<_> = ALL_FAMILIES.iter().map(|f| serde_json::json!({
        "id": f.id(), "label": f.label(), "full_size": f.full_size(),
        "accepted_file_sizes": f.remaps().iter().map(|r| r.source_len(f.full_size())).collect::<Vec<_>>(),
        "write_support": write_note(*f),
    })).collect();
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

/// Verify a BIN. With no family, every family whose layout resolves is reported.
#[tauri::command(async)]
pub fn gm_checksum_verify(bin_bytes: Vec<u8>, family: Option<String>) -> Result<String, String> {
    let reports = match family.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => {
            let f = Family::from_id(id).ok_or_else(|| format!("Unknown GM checksum family '{}'", id))?;
            vec![verify_family(f, &bin_bytes)?]
        }
        None => identify(&bin_bytes),
    };
    serde_json::to_string(&serde_json::json!({
        "reports": reports,
        "matched": reports.iter().filter(|r| r.all_valid).map(|r| r.family).collect::<Vec<_>>(),
        "note": if reports.is_empty() { "No supported GM family layout resolves on this file." } else { "Offline checksum check. No ECU is contacted and no write gate changes." },
    })).map_err(|e| e.to_string())
}

/// In-memory correction for export (Save As). Never flashes, never loosens a gate.
#[tauri::command(async)]
pub fn gm_checksum_fix_export(bin_bytes: Vec<u8>, family: String) -> Result<String, String> {
    let f = Family::from_id(&family).ok_or_else(|| format!("Unknown GM checksum family '{}'", family))?;
    let (bytes, rep, changed) = fix_family(f, &bin_bytes)?;
    serde_json::to_string(&serde_json::json!({
        "bytes": bytes, "report": rep, "words_changed": changed,
        "note": format!("Corrected copy for export only ({}). Output is the full 0x{:X}-byte layout. {}", rep.remap, bytes.len(), rep.write_support),
    })).map_err(|e| e.to_string())
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known_vectors() {
        assert_eq!(crc16_arc(b"123456789"), 0xBB3D);
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32_ieee(b""), 0);
    }

    fn def(method: Method, addr: usize, bytes: usize, comp: Complement, swap: bool, blocks: Vec<(usize, usize)>) -> CsDef {
        CsDef { segment: "T".into(), which: "cs1", method, addr, bytes, complement: comp, swap, msb: true, blocks, exclude: vec![] }
    }

    #[test]
    fn method_vectors_hand_derived() {
        let buf = [0x12, 0x34, 0x00, 0x00, 0xAB, 0xCD, 0x00, 0x01];
        // word sum over 0x1234 + 0xABCD + 0x0001 (field at 2..4 skipped) = 0xBE02
        let w = def(Method::WordSum16, 2, 2, Complement::None, false, vec![(0, 7)]);
        assert_eq!(compute(&buf, &w).unwrap(), 0xBE02);
        let w2 = CsDef { complement: Complement::Twos, ..w.clone() };
        assert_eq!(compute(&buf, &w2).unwrap(), 0x41FE);
        let w1 = CsDef { complement: Complement::Ones, ..w.clone() };
        assert_eq!(compute(&buf, &w1).unwrap(), 0x41FD);
        let ws = CsDef { swap: true, ..w.clone() };
        assert_eq!(compute(&buf, &ws).unwrap(), 0x02BE);
        let b = def(Method::ByteSum, 2, 2, Complement::None, false, vec![(0, 7)]);
        assert_eq!(compute(&buf, &b).unwrap(), 0x12 + 0x34 + 0xAB + 0xCD + 0x01);
        let d = def(Method::DwordSum32, 0, 4, Complement::None, false, vec![(0, 7)]);
        assert_eq!(compute(&buf, &d).unwrap(), 0xABCD_0001);
        // Excluded range is cut out of the covering block.
        let x = CsDef { exclude: vec![(4, 5)], ..w.clone() };
        assert_eq!(compute(&buf, &x).unwrap(), 0x1235);
    }

    #[test]
    fn crc_field_skipped_mid_block() {
        let mut buf = b"1234XX56789".to_vec();
        let c = def(Method::Crc16Arc, 4, 2, Complement::None, false, vec![(0, 10)]);
        assert_eq!(compute(&buf, &c).unwrap(), 0xBB3D);
        buf[4] = 0;
        assert_eq!(compute(&buf, &c).unwrap(), 0xBB3D);
    }

    #[test]
    fn block_past_end_is_error_not_truncated() {
        let c = def(Method::ByteSum, 0, 1, Complement::None, false, vec![(0, 10)]);
        assert!(compute(&[0u8; 4], &c).is_err());
    }

    fn put32(b: &mut [u8], at: usize, v: u32) { b[at..at + 4].copy_from_slice(&v.to_be_bytes()); }
    fn fill(b: &mut [u8], seed: u32) {
        let mut x = seed;
        for v in b.iter_mut() { x = x.wrapping_mul(1_103_515_245).wrapping_add(12345); *v = (x >> 16) as u8; }
    }

    /// Synthetic image (no calibration data) with a valid pointer table for `f`.
    fn synthetic(f: Family) -> Vec<u8> {
        let n = f.full_size();
        let mut b = vec![0u8; n];
        fill(&mut b, n as u32 ^ 0x5A5A);
        let set = |b: &mut Vec<u8>, at: usize, s: usize, e: usize| { put32(b, at, s as u32); put32(b, at + 4, e as u32); };
        match f {
            Family::P01P59 => {
                put32(&mut b, 0x510, 0x7FFFD);
                let segs = [(0x8000, 0x13FFF), (0x14000, 0x16DFF), (0x16E00, 0x1BDFF), (0x1BE00, 0x1C7FF), (0x1C800, 0x1E51F), (0x1E520, 0x1EE9F), (0x1EEA0, 0x1EF9F)];
                for (i, (s, e)) in segs.iter().enumerate() { set(&mut b, 0x514 + 8 * i, *s, *e); }
                put32(&mut b, 0x504, 12212156);
            }
            Family::P08P10 => {
                for (i, (s, e)) in [(0x8000, 0x10FFF), (0x11000, 0x16FFF), (0x17000, 0x171FF), (0x17200, 0x17FFF)].iter().enumerate() { set(&mut b, 0x54E + 8 * i, *s, *e); }
            }
            Family::P04V6 => {
                b[n - 4] = 0xA5; b[n - 3] = 0x5A;
                // unique load of 0x7FFFA, then the same opcode with the CS address
                for i in 0..n - 6 { if b[i] == 0x26 && b[i + 1] == 0x39 { b[i] = 0; } }
                b[0x1000..0x1006].copy_from_slice(&[0x26, 0x39, 0, 7, 0xFF, 0xFA]);
                b[0x1014..0x1016].copy_from_slice(&[0x26, 0x39]);
                put32(&mut b, 0x1016, 0x6FBFA);
            }
            Family::Ls1P30 => {}
            Family::E38 => {
                let segs = [(0x10024, 0x10000, 0x1BFFFF), (0x10048, 0x1C0000, 0x1C134F), (0x1006B, 0x1C1350, 0x1C3573), (0x1008E, 0x1C3574, 0x1C3917), (0x100B1, 0x1C3918, 0x1CD373), (0x100D4, 0x1CD374, 0x1FFFFF)];
                for (x, s, e) in segs {
                    set(&mut b, x, s, e);
                    set(&mut b, x + 8, s + 2, s + 0x1D);
                    set(&mut b, x + 16, s + 0x20, e);
                    set(&mut b, x + 24, s + 2, e);
                }
                b[0x414..0x418].fill(0xFF); // no boot block: must be skipped, not guessed
            }
            Family::E92 => {
                set(&mut b, 0xC0126, 0xC0000, 0x3FFFFF);
                set(&mut b, 0xC0136, 0xC0000, 0xC00FF); set(&mut b, 0xC013E, 0xC0102, 0xC011F); set(&mut b, 0xC0146, 0xC0122, 0x3FFFFF);
                set(&mut b, 0xC014E, 0xC0000, 0xC00FF); set(&mut b, 0xC0156, 0xC0102, 0x3FFFFF);
                for (x, s, e) in [(0xC0162, 0x40000, 0x42FFF), (0xC0186, 0x43000, 0x47FFF), (0xC01AA, 0x48000, 0x48FFF), (0xC01CE, 0x49000, 0x5FFFF), (0xC01F2, 0x60000, 0xBFFFF)] {
                    set(&mut b, x, s, e); set(&mut b, x + 8, s + 2, s + 0x1F); set(&mut b, x + 16, s + 0x22, e); set(&mut b, x + 24, s + 2, e);
                }
                b[0xFDA5..0xFDA9].fill(0xFF);
            }
            Family::T43 => {
                set(&mut b, 0x60022, 0x30000, 0x5FFFF);
                for (i, (s, e)) in [(0x30000, 0x5FFFF), (0x60002, 0x6001B), (0x6001E, 0x11FFFF), (0x120000, 0x1BFFFF)].iter().enumerate() { set(&mut b, 0x60032 + 8 * i, *s, *e); }
                for (i, (s, e)) in [(0x30000, 0x5FFFF), (0x60002, 0x11FFFF), (0x120000, 0x1BFFFF)].iter().enumerate() { set(&mut b, 0x60052 + 8 * i, *s, *e); }
                for (x, s, e) in [(0x6006E, 0x1C0000, 0x1FCB57), (0x60091, 0x1FCB58, 0x1FEB57), (0x600B4, 0x1FEB58, 0x1FFFFF)] {
                    set(&mut b, x, s, e); set(&mut b, x + 8, s + 2, s + 0x1B); set(&mut b, x + 16, s + 0x1E, e); set(&mut b, x + 24, s + 2, e);
                }
                b[0x2A022..0x2A026].fill(0xFF);
            }
            Family::DuramaxE54 => {
                set(&mut b, 0x2000A, 0x20000, 0x6FFFF);
                for (i, (s, e)) in [(0x8000, 0x19FFF), (0x1A000, 0x1C7FF), (0x1C800, 0x1DFFF), (0x1E000, 0x1EFFF), (0x1F000, 0x1FFEF)].iter().enumerate() { set(&mut b, 0x20012 + 8 * i, *s, *e); }
            }
            Family::DuramaxE60 => {
                set(&mut b, 0x4000A, 0x40000, 0xBFFFF);
                for (i, (s, e)) in [(0x8000, 0x1FFFF), (0x20000, 0x2E3FF), (0x2E400, 0x2F3FF), (0x2F400, 0x2F9FF), (0x2FA00, 0x2FDFF), (0x2FE00, 0x2FFFF)].iter().enumerate() { set(&mut b, 0x40012 + 8 * i, *s, *e); }
            }
            Family::VortecBlackbox9697 => {
                set(&mut b, 0x40C, 0x0, 0x38FA9);
                for (i, (s, e)) in [(0x38FAA, 0x3DBDB), (0x3DBDC, 0x3DBF1), (0x3DBF2, 0x3DC1F), (0x3DC20, 0x3DC57), (0x3FFE0, 0x3FFFD), (0x3DC58, 0x3FFDF)].iter().enumerate() { set(&mut b, 0x414 + 8 * i, *s, *e); }
            }
        }
        b
    }

    fn expected_count(f: Family) -> usize {
        match f {
            Family::P01P59 => 8, Family::P08P10 => 5, Family::P04V6 | Family::Ls1P30 => 1,
            Family::E38 | Family::E92 => 12, Family::T43 => 8, Family::DuramaxE54 => 6, Family::DuramaxE60 | Family::VortecBlackbox9697 => 7,
        }
    }

    #[test]
    fn every_family_corrupt_then_repair_in_memory() {
        for f in ALL_FAMILIES {
            let raw = synthetic(f);
            let (fixed, rep, _) = fix_family(f, &raw).unwrap_or_else(|e| panic!("{:?}: {}", f, e));
            assert_eq!(rep.total, expected_count(f), "{:?}", f);
            assert!(rep.all_valid, "{:?}", f);
            assert_eq!(fixed.len(), f.full_size());
            // Corrupt one byte inside the first checksum's data.
            let layout = resolve(f, &fixed).unwrap();
            let c0 = &layout.checksums[0];
            let (s, e) = *c0.blocks.last().unwrap();
            let mut pos = (s + e) / 2;
            if (c0.addr..c0.addr + c0.bytes).contains(&pos) { pos = s; }
            let mut bad = fixed.clone();
            bad[pos] ^= 0x5A;
            let r = verify_family(f, &bad).unwrap();
            assert!(!r.all_valid, "{:?}: corruption not detected", f);
            let (healed, r2, changed) = fix_family(f, &bad).unwrap();
            assert!(r2.all_valid && changed > 0, "{:?}", f);
            assert_eq!(verify_family(f, &healed).unwrap().passed, expected_count(f));
            // The source buffer is never modified (fix is a copy).
            assert_ne!(bad, healed);
        }
    }

    #[test]
    fn missing_boot_block_is_reported_not_guessed() {
        let r = verify_family(Family::E38, &fix_family(Family::E38, &synthetic(Family::E38)).unwrap().0).unwrap();
        assert!(r.notes.iter().any(|n| n.contains("BootBlock not checked")));
    }

    #[test]
    fn short_files_are_rebuilt() {
        // E38 read without the 64K boot block
        let full = fix_family(Family::E38, &synthetic(Family::E38)).unwrap().0;
        let short = full[0x10000..].to_vec();
        let r = verify_family(Family::E38, &short).unwrap();
        assert!(r.all_valid && r.remap.contains("boot block"));
        // TunerCat P01 layout round trip
        let p = fix_family(Family::P01P59, &synthetic(Family::P01P59)).unwrap().0;
        let mut tc = vec![0u8; 0x7C000];
        tc[..0x4000].copy_from_slice(&p[..0x4000]);
        tc[0x4000..0x64000].copy_from_slice(&p[0x20000..0x80000]);
        tc[0x64000..].copy_from_slice(&p[0x8000..0x20000]);
        let mut rebuilt = Remap::TunerCatP01.apply(&tc, 0x80000).unwrap();
        rebuilt[0x4000..0x8000].copy_from_slice(&p[0x4000..0x8000]);
        assert_eq!(rebuilt, p);
    }

    #[test]
    fn p01_agrees_with_existing_checksum_module() {
        let fixed = fix_family(Family::P01P59, &synthetic(Family::P01P59)).unwrap().0;
        let old = crate::checksum::validate_checksums(&fixed).unwrap();
        assert!(old.all_valid, "existing P01 validator disagrees: {:?}", old.method_used);
        let new = verify_family(Family::P01P59, &fixed).unwrap();
        assert_eq!(old.regions.len(), new.total);
        for (o, n) in old.regions.iter().zip(&new.checksums) {
            assert_eq!(format!("0x{:X}", o.cs_offset), n.addr);
            assert_eq!(format!("0x{:04X}", o.original_cs), n.stored);
        }
        // And the existing corrector produces the same bytes as ours.
        let mut bad = fixed.clone();
        bad[0x9000] ^= 0x11;
        assert_eq!(crate::checksum::correct_checksums(&bad).unwrap().data, fix_family(Family::P01P59, &bad).unwrap().0);
    }

    #[test]
    fn p01_private_dump_agrees_when_present() {
        let Ok(path) = std::env::var("TIV_P01_BIN") else { return };
        let Ok(data) = std::fs::read(path) else { return };
        let old = crate::checksum::validate_checksums(&data).unwrap();
        let new = verify_family(Family::P01P59, &data).unwrap();
        assert_eq!(old.all_valid, new.all_valid);
        assert_eq!(old.valid_count, new.passed);
    }

    #[test]
    fn identify_picks_the_matching_family() {
        let bb = fix_family(Family::VortecBlackbox9697, &synthetic(Family::VortecBlackbox9697)).unwrap().0;
        let ids = identify(&bb);
        assert_eq!(ids.first().map(|r| r.family), Some("GM_VORTEC_96_97_BLACKBOX"));
        assert!(identify(&[0u8; 1234]).is_empty());
    }

    #[test]
    fn commands_never_mention_enabling_writes() {
        let bb = fix_family(Family::DuramaxE54, &synthetic(Family::DuramaxE54)).unwrap().0;
        let v: serde_json::Value = serde_json::from_str(&gm_checksum_verify(bb.clone(), Some("GM_DURAMAX_E54".into())).unwrap()).unwrap();
        assert_eq!(v["reports"][0]["write_support"], WRITE_NOTE_OTHER);
        let x: serde_json::Value = serde_json::from_str(&gm_checksum_fix_export(bb, "duramax_e54".into()).unwrap()).unwrap();
        assert!(x["note"].as_str().unwrap().contains("export only"));
        assert!(gm_checksum_fix_export(vec![0u8; 16], "GM_E38".into()).is_err());
        assert!(gm_checksum_verify(vec![], Some("NOPE".into())).is_err());
    }

    /// Opt-in corpus check. Point TIV_GM_CS_CORPUS at a folder holding the
    /// stock images (never committed). Files are matched by SHA-256 only; any
    /// other file in the folder is ignored. With all 33 images present this
    /// must reproduce 267/267.
    #[test]
    fn corpus_reproduces_stored_checksums_when_present() {
        use sha2::{Digest, Sha256};
        let Ok(dir) = std::env::var("TIV_GM_CS_CORPUS") else { return };
        let mut by_hash = std::collections::HashMap::new();
        for e in std::fs::read_dir(&dir).expect("TIV_GM_CS_CORPUS is not a folder").flatten() {
            let p = e.path();
            let Ok(meta) = std::fs::metadata(&p) else { continue };
            // Only sizes a supported layout accepts are even read.
            let ok_size = ALL_FAMILIES.iter().any(|f| f.remaps().iter().any(|r| r.source_len(f.full_size()) as u64 == meta.len()));
            if !meta.is_file() || !ok_size { continue; }
            let data = std::fs::read(&p).unwrap();
            by_hash.insert(format!("{:x}", Sha256::digest(&data)), data);
        }
        let (mut images, mut passed, mut total) = (0, 0, 0);
        for (sha, fam, n) in corpus::EXPECTED {
            let Some(raw) = by_hash.get(*sha) else { continue };
            let f = Family::from_id(fam).unwrap();
            let r = verify_family(f, raw).unwrap_or_else(|e| panic!("{}: {}", &sha[..12], e));
            assert_eq!(r.total, *n, "{} {}: checksum count", fam, &sha[..12]);
            assert!(r.all_valid, "{} {}: {}/{}", fam, &sha[..12], r.passed, r.total);
            // corrupt -> fail, fix -> pass
            let rm = f.remaps().iter().copied().find(|rm| rm.label() == r.remap).unwrap();
            let buf = rm.apply(raw, f.full_size()).unwrap();
            let l = resolve(f, &buf).unwrap();
            let c0 = &l.checksums[0];
            let (s, e) = *c0.blocks.last().unwrap();
            let mut pos = (s + e) / 2;
            if (c0.addr..c0.addr + c0.bytes).contains(&pos) { pos = s; }
            let mut bad = buf.clone();
            bad[pos] ^= 0x5A;
            assert!(!verify_family(f, &bad).unwrap().all_valid);
            assert!(fix_family(f, &bad).unwrap().1.all_valid);
            images += 1; passed += r.passed; total += r.total;
            eprintln!("{:<26} {} {}/{} os={:?} remap={}", fam, &sha[..12], r.passed, r.total, r.os_id, r.remap);
        }
        eprintln!("TOTAL {}/{} checksums on {} images", passed, total, images);
        assert_eq!(passed, total);
        if images == corpus::EXPECTED.len() { assert_eq!(total, 267); }
    }

    mod corpus {
        /// (SHA-256 of the file as distributed, family, stored checksums).
        /// Hashes only: the images themselves are never committed.
        pub const EXPECTED: &[(&str, &str, usize)] = &[
            ("c55a8f9afad41896cdac9b3d21f1995939e865d7b96769b7303425cde6778857", "GM_P01_P59", 8),
            ("bc4839d545720ac3a3521fb3b7722627a5dff8cdbb11611a874fc8e1e668227a", "GM_P01_P59", 8),
            ("fc64acb25bcbfdde40f377a68ef299b9922fda8fbe84abb6ca5c19c22b06f385", "GM_P01_P59", 8),
            ("a2d37c40fbbcf815cf9af216079ff885ad1de1bc901faa02810bff4961a3631a", "GM_P08_P10", 5),
            ("8482ff753e13efd3b1b0abf30f1f2fdfd20091e9bd58e81682cbaa89668036f1", "GM_P04_V6", 1),
            ("83d5e826230c07c3620e38a927bff2e965baf07c3675a35b3c282cf3c4067d5e", "GM_LS1_97_98", 1),
            ("1121030680322a27fcc55f4b6b0757319495a80a2baeb3f1dbb0d8239ad94417", "GM_LS1_97_98", 1),
            ("e239e95524d86c08b28df7486124a57b5deb8d6a0105b3e8184d05d6ed19b168", "GM_E38", 12),
            ("841e7b8dd6b6e49b3eb3ae5debbfcaf688e6b44605cb0a287358d85ef1288c8a", "GM_E38", 12),
            ("0a9fcb9d969d5db37ae58e4b38e3417b700319f7392f704671ec74ae6093cc7a", "GM_E38", 14),
            ("a78f1d15f2e329f6aa992260574625322166b4a5e3ec8461f99e7e047061fa5a", "GM_T43", 8),
            ("29a937c4e6046d0b6da56328a9535c64e2fe14f1bec22160c1c4adb3c3a3706a", "GM_T43", 8),
            ("550638b320720bb368dd58db07efec3fa88a6b1c53aeb3041285be42f08ce800", "GM_DURAMAX_E54", 6),
            ("308c49e9639c6f87574226d8f2d20021f19a90b3738e2e1abeeb219b2f928d06", "GM_DURAMAX_E60", 7),
            ("72fbf5da4af8137265323a0cc82a3e9e792bc00aaab411ed33187cc7e81a6797", "GM_DURAMAX_E60", 7),
            ("63a26f5de0be961a8fc300f18efcaa06f1ca9c3131b779568dc5b6e6169fdc33", "GM_DURAMAX_E60", 7),
            ("1f02b3f2536368a2799b925e1940cb9c7daa035ae3573854f249f4f7a5a0aca9", "GM_E92", 12),
            ("3c02afec9469aa031b59190239d471fdcf73aea5e0b780c79980760c0222f96f", "GM_E92", 12),
            ("3af94d3198ac3488cd93b9ebcc268d634bd2c063a2aab0fe5fe35180178d9b8e", "GM_E92", 12),
            ("423db5f4fa522ec08db7715b19312a51adb88a0172073bbacb852e39e27a0ac2", "GM_E92", 12),
            ("dae20c484d90c92626daa22740e5bb3044adcc10bed43cdf8cd61ad1c6de86bc", "GM_E92", 12),
            ("456e3e3f204e79c223bb11f04b4814c56328b87ef39b62ec7013cb9632fdb31c", "GM_E92", 12),
            ("05abbd2c0377c1fc824bcc6f5c5364824128970510a168e6f4d43cca49db281d", "GM_E92", 12),
            ("4526db2a428255186d2125fe8d340875ebb8c50d34ca3a44a5b8f2007177c48b", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("c12bfea35a2fe0a218b3e507e3f791b5d0b117f69417d4f7605f1b288b377126", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("b11bbc64696f8d53561f02b7c78aa66c9a69603c42da8388cbf4815a0fd47bc8", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("619f9fc9e98c294ad0ff3655a523430b90eec78234dffe662661a45a8e06beb4", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("ef6b46f5907547343f0129c5ca9a2e97d655b148bf2ab313d55c214fb1337caf", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("6d8719253a2166a3ecde725e11c7977d7f252f0e22b4f0cd08e99a7574759395", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("c5d8ddf81bb95c5e8829862eb29acddd6bad7f392df5e07b8ad30a94d90233c2", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("cf59519669da9dccdfd9c2f786780aae45a278bd7a2a5609a0af17e70bad0408", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("c17e25de990e0c8b732dee3e2d665b05847c7f3f30373b99601e3f3195d8706e", "GM_VORTEC_96_97_BLACKBOX", 7),
            ("90a4af5842ad796703a5b40df05e8b2aabe325f000533ca5ce8d94d3a70af7d6", "GM_VORTEC_96_97_BLACKBOX", 7),
        ];
    }
}
