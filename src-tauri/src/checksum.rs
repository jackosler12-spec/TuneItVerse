// checksum.rs — Multi-ECU support: P01 additive + EDC16C41 multipoint CRC32 + additive fallback
//
// v3.26.0: Honda and P59 OS on a P01-sized image are report-only. Correction stays fail-closed.

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionResult {
    pub name: String,
    pub block: u8,
    pub cs_offset: usize,
    pub original_cs: u32,
    pub corrected_cs: u32,
    pub was_valid: bool,
    pub is_valid: bool,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksumReport {
    pub regions: Vec<RegionResult>,
    pub valid_count: usize,
    pub fixed_count: usize,
    pub failed_count: usize,
    pub all_valid: bool,
    pub ecu_family: String,
    pub method_used: String,
    /// GM CVNs (CS2, CRC16) per segment. Computed after CS1; not stored in the image.
    #[serde(default)]
    pub cvns: Vec<SegmentCvn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentCvn {
    pub name: String,
    pub segment_nr: u8,
    pub part_number: u32,
    pub version: String,
    pub cvn: String,
    /// Stock CVN from reference/stockcvn.xml for (PN, segment), if listed.
    pub stock_cvn: Option<String>,
    pub matches_stock: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectedCal {
    pub data: Vec<u8>,
    pub report: ChecksumReport,
}

pub use crate::checksum_sizes::{
    BLOCK_SIZE, CAL_IMAGE_SIZE, EDC16_FLASH_SIZE, P01_FULL_IMAGE_SIZE, is_p01_size, p01_block_count,
};

#[derive(Debug, Clone)]
pub struct RegionDescriptor {
    pub name: &'static str,
    pub start: usize,
    pub end: usize,
    pub cs_offset: usize,
}

pub const EDC16_CRC32_REGIONS: [RegionDescriptor; 7] = [
    RegionDescriptor { name: "Bootloader / Low OS", start: 0x00000, end: 0x1FFFF, cs_offset: 0x1FFFC },
    RegionDescriptor { name: "Main Program Code", start: 0x20000, end: 0x7FFFF, cs_offset: 0x7FFFC },
    RegionDescriptor { name: "Primary Cal - Driver Wish / IQ / Timing / Rail", start: 0x80000, end: 0xBFFFF, cs_offset: 0xBFFFC },
    RegionDescriptor { name: "Secondary Cal - Boost / VGT / EGR / Smoke", start: 0xC0000, end: 0xFFFFF, cs_offset: 0xFFFFC },
    RegionDescriptor { name: "Extended Maps & Limiters", start: 0x100000, end: 0x13FFFF, cs_offset: 0x13FFFC },
    RegionDescriptor { name: "Adaptations / Coding / Misc", start: 0x140000, end: 0x17FFFF, cs_offset: 0x17FFFC },
    RegionDescriptor { name: "End / Security / Global CS Area", start: 0x180000, end: 0x1FFFFF, cs_offset: 0x1FFFFC },
];

pub const EDC16_ADDITIVE_REGIONS: [RegionDescriptor; 7] = [
    RegionDescriptor { name: "Bootloader / Low OS", start: 0x00000, end: 0x1FFFF, cs_offset: 0x1FFFE },
    RegionDescriptor { name: "Main Program Code", start: 0x20000, end: 0x7FFFF, cs_offset: 0x7FFFE },
    RegionDescriptor { name: "Primary Cal - Driver Wish / IQ / Timing / Rail", start: 0x80000, end: 0xBFFFF, cs_offset: 0xBFFFE },
    RegionDescriptor { name: "Secondary Cal - Boost / VGT / EGR / Smoke", start: 0xC0000, end: 0xFFFFF, cs_offset: 0xFFFFE },
    RegionDescriptor { name: "Extended Maps & Limiters", start: 0x100000, end: 0x13FFFF, cs_offset: 0x13FFFE },
    RegionDescriptor { name: "Adaptations / Coding / Misc", start: 0x140000, end: 0x17FFFF, cs_offset: 0x17FFFE },
    RegionDescriptor { name: "End / Security / Global CS Area", start: 0x180000, end: 0x1FFFFF, cs_offset: 0x1FFFFE },
];

const CRC32_POLY: u32 = 0xEDB88320;
const CRC32_INIT: u32 = 0xFFFFFFFF;
const CRC32_XOROUT: u32 = 0xFFFFFFFF;

fn make_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for i in 0..256 {
        let mut c = i as u32;
        for _ in 0..8 {
            if c & 1 != 0 { c = CRC32_POLY ^ (c >> 1); } else { c >>= 1; }
        }
        table[i] = c;
    }
    table
}

fn crc32(data: &[u8]) -> u32 {
    let table = make_crc32_table();
    let mut crc = CRC32_INIT;
    for &b in data {
        let idx = ((crc ^ b as u32) & 0xFF) as usize;
        crc = table[idx] ^ (crc >> 8);
    }
    crc ^ CRC32_XOROUT
}

fn read_u32_le(data: &[u8], off: usize) -> u32 {
    if off + 3 >= data.len() { return 0; }
    u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]])
}
fn write_u32_le(data: &mut [u8], off: usize, val: u32) {
    if off + 3 >= data.len() { return; }
    let bytes = val.to_le_bytes();
    data[off] = bytes[0]; data[off+1] = bytes[1]; data[off+2] = bytes[2]; data[off+3] = bytes[3];
}
fn read_u16_be(data: &[u8], off: usize) -> u16 {
    if off + 1 >= data.len() { return 0; }
    ((data[off] as u16) << 8) | (data[off + 1] as u16)
}
fn write_u16_be(data: &mut [u8], off: usize, val: u16) {
    if off + 1 >= data.len() { return; }
    data[off] = (val >> 8) as u8;
    data[off + 1] = val as u8;
}
fn region_sum16(block: &[u8], start: usize, end: usize) -> u16 {
    let mut sum: u16 = 0;
    let mut i = start;
    while i <= end && i + 1 < block.len() {
        sum = sum.wrapping_add(read_u16_be(block, i));
        i += 2;
    }
    sum
}

fn validate_crc32_region(data: &[u8], region: &RegionDescriptor) -> bool {
    let full = &data[region.start..=region.end.min(data.len()-1)];
    crc32(full) == 0
}

fn correct_crc32_region(data: &mut [u8], region: &RegionDescriptor) -> Result<u32, String> {
    if region.cs_offset + 3 > region.end || region.cs_offset + 3 >= data.len() {
        return Err(format!("CS offset out of range for region '{}'", region.name));
    }
    write_u32_le(data, region.cs_offset, 0);
    let base_crc = crc32(&data[region.start..=region.end.min(data.len()-1)]);
    let new_cs = base_crc;
    write_u32_le(data, region.cs_offset, new_cs);
    let final_crc = crc32(&data[region.start..=region.end.min(data.len()-1)]);
    if final_crc != 0 && final_crc != new_cs {
        let alt = !new_cs;
        write_u32_le(data, region.cs_offset, alt);
        let final2 = crc32(&data[region.start..=region.end.min(data.len()-1)]);
        if final2 != 0 {
            return Err(format!("CRC32 post-correction verify failed for '{}' (got 0x{:08X})", region.name, final2));
        }
        return Ok(alt);
    }
    Ok(new_cs)
}

fn validate_additive_region(block: &[u8], region: &RegionDescriptor) -> bool {
    region_sum16(block, region.start, region.end) == 0
}

fn correct_additive_region(block: &mut [u8], region: &RegionDescriptor) -> Result<u16, String> {
    if region.end.saturating_sub(region.start) < 2 { return Err("region too small".into()); }
    let sum_excl = region_sum16(block, region.start, region.end.saturating_sub(2));
    let new_cs = 0u16.wrapping_sub(sum_excl);
    write_u16_be(block, region.cs_offset, new_cs);
    if region_sum16(block, region.start, region.end) != 0 {
        return Err(format!("Post-correction verify failed for region '{}'", region.name));
    }
    Ok(new_cs)
}

// ── GM P01 (512KB) segment checksums ───────────────────────────────────────
// Layout source: reference/P01-P59.xml (UniversalPatcher SegmentConfig):
//   OS:   wordsum, two's complement (CS1Complement=2), CS word at 0x500,
//         blocks 0x0-0x4FF, 0x502-0x3FFF, 0x20000-[u32 end @0x510]
//   Cal:  7 segments, [u32 start, u32 end] pairs at 0x514 + 8*i
//         (EngineCal, EngineDiag, TransCal, TransDiag, Fuel, System, Speedometer);
//         CS word at segment offset 0, wordsum two's complement over the rest.
// Ranges are inclusive. Big-endian 16-bit words.

pub const P01_OS_CS_ADDR: usize = 0x500;
pub const P01_OS_END_PTR: usize = 0x510;
pub const P01_SEG_TABLE: usize = 0x514;
pub const P01_CAL_SEGMENT_NAMES: [&str; 7] = ["EngineCal", "EngineDiag", "TransCal", "TransDiag", "Fuel", "System", "Speedometer"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P01Segment {
    pub name: &'static str,
    pub cs_addr: usize,
    /// Inclusive byte ranges summed (excluding the CS word).
    pub blocks: Vec<(usize, usize)>,
}

fn read_u32_be(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Read the P01 segment layout from the image's own OS tables. Errors if the
/// table is missing or not sane (no guessing).
pub fn p01_segments(data: &[u8]) -> Result<Vec<P01Segment>, String> {
    if data.len() != P01_FULL_IMAGE_SIZE {
        return Err(format!("P01 segment checksums need the full 512KB image (got {} bytes); a cal-only file has no segment table.", data.len()));
    }
    let os_end = read_u32_be(data, P01_OS_END_PTR).unwrap() as usize;
    if !(0x20000..data.len()).contains(&os_end) {
        return Err(format!("P01 OS end pointer @0x510 = 0x{:X} is not sane. Not a P01 OS layout.", os_end));
    }
    let mut segs = vec![P01Segment { name: "OS", cs_addr: P01_OS_CS_ADDR, blocks: vec![(0x0, 0x4FF), (0x502, 0x3FFF), (0x20000, os_end)] }];
    let mut prev_end = 0x7FFFusize;
    for (i, name) in P01_CAL_SEGMENT_NAMES.iter().enumerate() {
        let o = P01_SEG_TABLE + 8 * i;
        let start = read_u32_be(data, o).unwrap() as usize;
        let end = read_u32_be(data, o + 4).unwrap() as usize;
        if start <= prev_end || end <= start + 2 || end >= 0x20000 || start % 2 != 0 || (end + 1) % 2 != 0 {
            return Err(format!("P01 segment table entry {} ({}) @0x{:X} = 0x{:X}-0x{:X} is not sane.", i, name, o, start, end));
        }
        segs.push(P01Segment { name, cs_addr: start, blocks: vec![(start + 2, end)] });
        prev_end = end;
    }
    Ok(segs)
}

fn seg_sum(data: &[u8], seg: &P01Segment) -> u16 {
    seg.blocks.iter().fold(0u16, |acc, &(a, b)| acc.wrapping_add(region_sum16(data, a, b)))
}

fn p01_expected_cs(data: &[u8], seg: &P01Segment) -> u16 {
    0u16.wrapping_sub(seg_sum(data, seg))
}

/// GM P01 OS part number (u32 BE @0x504, PNAddr in reference/P01-P59.xml), only
/// when the image's OS segment checksum validates — otherwise None.
pub fn p01_binary_os_id(data: &[u8]) -> Option<u32> {
    let segs = p01_segments(data).ok()?;
    let os = &segs[0];
    if read_u16_be(data, os.cs_addr) != p01_expected_cs(data, os) { return None; }
    read_u32_be(data, 0x504)
}

// ── GM CS2 / CVN ───────────────────────────────────────────────────────────
// reference/P01-P59.xml: Checksum2Method=crc16, CS2Address empty (not stored),
// CS2SwapBytes=true, CVN=2 (CS2 is the CVN). CS2Blocks: OS 0-3FFF,20000-@510;
// each cal segment @514.. (whole segment incl. its CS1 word). So CS1 must be
// final before CS2 is computed. The CRC variant is not named in the XML; it was
// identified as CRC-16/ARC (poly 0x8005 reflected = 0xA001, init 0, xorout 0)
// because it reproduces the stock CVN 717A for OS 12225074 ver BF listed in
// reference/stockcvn.xml (line ~68300).

pub fn crc16_arc(chunks: &[&[u8]]) -> u16 {
    let mut c: u16 = 0;
    for ch in chunks {
        for &b in *ch {
            c ^= b as u16;
            for _ in 0..8 { c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 }; }
        }
    }
    c
}

fn stock_cvns() -> &'static std::collections::HashMap<(u32, u8), String> {
    static M: std::sync::OnceLock<std::collections::HashMap<(u32, u8), String>> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        let xml = include_str!("../../reference/stockcvn.xml");
        let mut m = std::collections::HashMap::new();
        for rec in xml.split("<CVN>").skip(1) {
            let tag = |t: &str| rec.split(&format!("<{}>", t)).nth(1).and_then(|r| r.split(&format!("</{}>", t)).next()).map(|v| v.trim().to_string());
            if let (Some(pn), Some(nr), Some(cvn)) = (tag("PN"), tag("SegmentNr"), tag("cvn")) {
                if let (Ok(pn), Ok(nr)) = (pn.parse::<u32>(), nr.parse::<u8>()) {
                    m.entry((pn, nr)).or_insert(cvn.to_ascii_uppercase());
                }
            }
        }
        m
    })
}

/// CVN (CS2) for every P01 segment, with stock comparison where known.
pub fn p01_cvns(data: &[u8]) -> Result<Vec<SegmentCvn>, String> {
    let segs = p01_segments(data)?;
    let mut out = vec![];
    for (i, seg) in segs.iter().enumerate() {
        let (crc, base) = if i == 0 {
            let end = seg.blocks[2].1;
            (crc16_arc(&[&data[0..0x4000], &data[0x20000..=end]]), 0x500usize)
        } else {
            let start = seg.cs_addr;
            let end = seg.blocks[0].1;
            (crc16_arc(&[&data[start..=end]]), start)
        };
        let cvn = crc.swap_bytes(); // CS2SwapBytes=true
        let segment_nr = data[base + 3];
        let part_number = read_u32_be(data, base + 4).unwrap_or(0);
        let version = String::from_utf8_lossy(&data[base + 8..base + 10]).to_string();
        let cvn_s = format!("{:04X}", cvn);
        let stock = stock_cvns().get(&(part_number, segment_nr)).cloned();
        let matches_stock = stock.as_ref().map(|s| s == &cvn_s);
        out.push(SegmentCvn { name: seg.name.to_string(), segment_nr, part_number, version, cvn: cvn_s, stock_cvn: stock, matches_stock });
    }
    Ok(out)
}

fn validate_p01_checksums(data: &[u8]) -> Result<ChecksumReport, String> {
    let segs = match p01_segments(data) {
        Ok(s) => s,
        Err(e) => return Ok(ChecksumReport { regions: vec![], valid_count: 0, fixed_count: 0, failed_count: 0, all_valid: false, ecu_family: "P01_0411".into(), method_used: format!("report-only: {}", e), cvns: vec![] }),
    };
    let mut regions = vec![];
    let (mut valid_count, mut failed_count) = (0, 0);
    for (i, seg) in segs.iter().enumerate() {
        let cs = read_u16_be(data, seg.cs_addr);
        let ok = cs == p01_expected_cs(data, seg);
        if ok { valid_count += 1 } else { failed_count += 1 }
        regions.push(RegionResult { name: seg.name.to_string(), block: i as u8, cs_offset: seg.cs_addr, original_cs: cs as u32, corrected_cs: cs as u32, was_valid: ok, is_valid: ok, method: "GM Wordsum16 (2's complement)".into() });
    }
    let cvns = p01_cvns(data).unwrap_or_default();
    Ok(ChecksumReport { regions, valid_count, fixed_count: 0, failed_count, all_valid: failed_count == 0, ecu_family: "P01_0411".into(), method_used: "GM P01 segment wordsum CS1 + CRC16 CVN CS2 (reference/P01-P59.xml)".into(), cvns })
}

fn correct_p01_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    let segs = p01_segments(data)?;
    let mut buf = data.to_vec();
    let mut regions = vec![];
    let (mut valid_count, mut fixed_count) = (0, 0);
    for (i, seg) in segs.iter().enumerate() {
        let cs = read_u16_be(&buf, seg.cs_addr);
        let want = p01_expected_cs(&buf, seg);
        let was_valid = cs == want;
        if was_valid { valid_count += 1 } else { write_u16_be(&mut buf, seg.cs_addr, want); fixed_count += 1; }
        regions.push(RegionResult { name: seg.name.to_string(), block: i as u8, cs_offset: seg.cs_addr, original_cs: cs as u32, corrected_cs: want as u32, was_valid, is_valid: true, method: "GM Wordsum16 (2's complement)".into() });
    }
    // CS1 words are final; CVNs (CS2) are recomputed over them (they cover the CS1 word).
    let post = validate_p01_checksums(&buf)?;
    if !post.all_valid { return Err("P01 post-correction verify failed".into()); }
    Ok(CorrectedCal { data: buf, report: ChecksumReport { regions, valid_count, fixed_count, failed_count: 0, all_valid: true, ecu_family: "P01_0411".into(), method_used: post.method_used, cvns: post.cvns } })
}

fn validate_edc16_crc32(data: &[u8]) -> Result<ChecksumReport, String> {
    if data.len() != EDC16_FLASH_SIZE { return Err(format!("Expected {} bytes for EDC16C41, got {}", EDC16_FLASH_SIZE, data.len())); }
    let mut regions = vec![];
    let mut valid_count = 0;
    let mut failed_count = 0;
    for region in &EDC16_CRC32_REGIONS {
        let was_valid = validate_crc32_region(data, region);
        let original_cs = read_u32_le(data, region.cs_offset);
        if was_valid { valid_count += 1; } else { failed_count += 1; }
        regions.push(RegionResult { name: region.name.to_string(), block: 0, cs_offset: region.cs_offset, original_cs, corrected_cs: original_cs, was_valid, is_valid: was_valid, method: "CRC32".to_string() });
    }
    Ok(ChecksumReport { regions, valid_count, fixed_count: 0, failed_count, all_valid: failed_count == 0, ecu_family: "EDC16C41".to_string(), method_used: "CRC32 multipoint".to_string(), cvns: vec![] })
}

fn correct_edc16_crc32(data: &[u8]) -> Result<CorrectedCal, String> {
    if data.len() != EDC16_FLASH_SIZE { return Err(format!("Expected {} bytes for EDC16, got {}", EDC16_FLASH_SIZE, data.len())); }
    let mut buf = data.to_vec();
    let mut results = vec![];
    let mut valid_count = 0;
    let mut fixed_count = 0;
    let mut failed_count = 0;
    for region in &EDC16_CRC32_REGIONS {
        let original_cs = read_u32_le(&buf, region.cs_offset);
        let was_valid = validate_crc32_region(&buf, region);
        let (corrected_cs, is_valid) = if was_valid {
            valid_count += 1; (original_cs, true)
        } else {
            match correct_crc32_region(&mut buf, region) {
                Ok(new_cs) => { fixed_count += 1; (new_cs, true) }
                Err(e) => { failed_count += 1; return Err(format!("Region '{}': {}", region.name, e)); }
            }
        };
        results.push(RegionResult { name: region.name.to_string(), block: 0, cs_offset: region.cs_offset, original_cs, corrected_cs, was_valid, is_valid, method: "CRC32".to_string() });
    }
    Ok(CorrectedCal { data: buf, report: ChecksumReport { regions: results, valid_count, fixed_count, failed_count, all_valid: failed_count == 0, ecu_family: "EDC16C41".to_string(), method_used: "CRC32 multipoint".to_string(), cvns: vec![] } })
}

pub fn validate_checksums(data: &[u8]) -> Result<ChecksumReport, String> {
    if crate::cs_guard::honda_blocks_p01_corrector(data) {
        return Ok(ChecksumReport {
            regions: vec![],
            valid_count: 0,
            fixed_count: 0,
            failed_count: 0,
            all_valid: false,
            ecu_family: "HONDA_KEIHIN".into(),
            method_used: "report-only: Honda OS on P01-sized image — P01 additive blocked".into(), cvns: vec![],
        });
    }
    if crate::cs_guard::p59_blocks_p01_corrector(data) {
        return Ok(ChecksumReport {
            regions: vec![],
            valid_count: 0,
            fixed_count: 0,
            failed_count: 0,
            all_valid: false,
            ecu_family: "GM_P59".into(),
            method_used: "report-only: P59 OS on P01-sized image — P01 additive blocked".into(), cvns: vec![],
        });
    }
    if is_p01_size(data.len()) { validate_p01_checksums(data) }
    else if data.len() == EDC16_FLASH_SIZE { validate_edc16_crc32(data) }
    else {
        Ok(ChecksumReport {
            regions: vec![],
            valid_count: 0,
            fixed_count: 0,
            failed_count: 0,
            all_valid: false,
            ecu_family: "UNKNOWN".into(),
            method_used: format!("report-only: no verified corrector for {} bytes", data.len()),
            cvns: vec![],
        })
    }
}

pub fn correct_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    if crate::cs_guard::honda_blocks_p01_corrector(data) {
        return Err("Honda OS string on this image. P01 additive correction is blocked.".into());
    }
    if crate::cs_guard::p59_blocks_p01_corrector(data) {
        return Err("P59 OS string on this image. P01 additive correction is blocked until measured P59 CS words exist.".into());
    }
    if is_p01_size(data.len()) { correct_p01_checksums(data) }
    else if data.len() == EDC16_FLASH_SIZE { correct_edc16_crc32(data) }
    else { Err(format!("Unsupported size for correction: {}", data.len())) }
}

pub fn correct_and_validate_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    let pre = validate_checksums(data)?;
    if pre.all_valid {
        return Ok(CorrectedCal {
            data: data.to_vec(),
            report: ChecksumReport {
                regions: pre.regions.into_iter().map(|mut r| { r.corrected_cs = r.original_cs; r }).collect(),
                valid_count: pre.valid_count, fixed_count: 0, failed_count: 0, all_valid: true,
                ecu_family: pre.ecu_family, method_used: pre.method_used, cvns: pre.cvns,
            },
        });
    }
    correct_checksums(data)
}

pub fn validate_bin_checksums_summary(data: &[u8]) -> Result<String, String> {
    let report = validate_checksums(data)?;
    let mut summary = format!("Checksum validation for {} ({} bytes) using {}\n", report.ecu_family, data.len(), report.method_used);
    summary += &format!("Regions checked: {}\n", report.regions.len());
    summary += &format!("Valid: {} | Fixed needed: {} | Failed: {}\n", report.valid_count, report.fixed_count, report.failed_count);
    summary += if report.all_valid { "OK All checksums VALID\n" } else { "WARN Some checksums INVALID - use correct_checksums() only when a verified routine exists\n" };
    for r in &report.regions {
        let status = if r.is_valid { "OK" } else { "BAD" };
        summary += &format!("  {} @0x{:06X}: orig=0x{:08X} corr=0x{:08X} [{}] (was valid: {})\n", status, r.cs_offset, r.original_cs, r.corrected_cs, r.method, r.was_valid);
    }
    Ok(summary)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    /// Synthetic 512KB P01-layout fixture (no real calibration data).
    pub(crate) fn synthetic_p01() -> Vec<u8> {
        let mut d = vec![0u8; P01_FULL_IMAGE_SIZE];
        for (i, b) in d.iter_mut().enumerate() { *b = (i.wrapping_mul(31) ^ (i >> 7)) as u8; }
        d[0x510..0x514].copy_from_slice(&0x7FFFDu32.to_be_bytes());
        let segs: [(u32, u32); 7] = [(0x8000, 0x13FFF), (0x14000, 0x16DFF), (0x16E00, 0x1BDFF), (0x1BE00, 0x1C7FF), (0x1C800, 0x1E51F), (0x1E520, 0x1EE9F), (0x1EEA0, 0x1EF9F)];
        for (i, (a, b)) in segs.iter().enumerate() {
            let o = 0x514 + 8 * i;
            d[o..o + 4].copy_from_slice(&a.to_be_bytes());
            d[o + 4..o + 8].copy_from_slice(&b.to_be_bytes());
        }
        correct_checksums(&d).unwrap().data
    }
    #[test]
    fn validate_unknown_size_is_report_only() {
        let r = validate_checksums(&vec![0u8; 100]).expect("unknown size must report, not error");
        assert_eq!(r.ecu_family, "UNKNOWN");
        assert!(r.method_used.contains("report-only"));
        assert!(correct_checksums(&vec![0u8; 100]).is_err());
    }
    #[test]
    fn p01_layout_from_image_tables() {
        let d = synthetic_p01();
        let segs = p01_segments(&d).unwrap();
        assert_eq!(segs.len(), 8);
        assert_eq!(segs[0].cs_addr, 0x500);
        assert_eq!(segs[0].blocks, vec![(0, 0x4FF), (0x502, 0x3FFF), (0x20000, 0x7FFFD)]);
        assert_eq!(segs[1].cs_addr, 0x8000);
        assert_eq!(segs[7].blocks, vec![(0x1EEA2, 0x1EF9F)]);
    }
    #[test]
    fn p01_pristine_validates_all_eight() {
        let r = validate_checksums(&synthetic_p01()).unwrap();
        assert!(r.all_valid, "{:?}", r);
        assert_eq!(r.valid_count, 8);
    }
    #[test]
    fn p01_cal_edit_changes_only_that_cs_word() {
        let orig = synthetic_p01();
        let mut dirty = orig.clone();
        dirty[0x9000] ^= 0x5A; // EngineCal
        let v = validate_checksums(&dirty).unwrap();
        assert_eq!(v.failed_count, 1);
        assert!(!v.regions[1].was_valid);
        let fixed = correct_checksums(&dirty).unwrap();
        assert!(validate_checksums(&fixed.data).unwrap().all_valid);
        let diffs: Vec<usize> = (0..orig.len()).filter(|&i| fixed.data[i] != orig[i]).collect();
        assert!(diffs.iter().all(|&i| i == 0x9000 || i == 0x8000 || i == 0x8001), "{:X?}", diffs);
        assert_eq!(fixed.report.fixed_count, 1);
    }
    #[test]
    fn p01_os_edit_fixes_os_word_only() {
        let orig = synthetic_p01();
        let mut dirty = orig.clone();
        dirty[0x30000] ^= 1;
        let fixed = correct_checksums(&dirty).unwrap();
        let diffs: Vec<usize> = (0..orig.len()).filter(|&i| fixed.data[i] != orig[i]).collect();
        assert!(diffs.iter().all(|&i| i == 0x30000 || i == 0x500 || i == 0x501), "{:X?}", diffs);
    }
    #[test]
    fn crc16_arc_check_value() {
        // Standard CRC-16/ARC check value for "123456789".
        assert_eq!(crc16_arc(&[b"123456789"]), 0xBB3D);
        assert_eq!(crc16_arc(&[b"1234", b"56789"]), 0xBB3D);
    }
    #[test]
    fn stock_cvn_table_has_12225074() {
        assert_eq!(stock_cvns().get(&(12225074, 1)).map(String::as_str), Some("717A"));
    }
    #[test]
    fn cvn_covers_cs1_and_follows_correction() {
        let orig = synthetic_p01();
        let before = p01_cvns(&orig).unwrap();
        assert_eq!(before.len(), 8);
        let mut dirty = orig.clone();
        dirty[0x9000] ^= 0x5A;
        let fixed = correct_checksums(&dirty).unwrap();
        // Report CVNs equal CVNs recomputed on the final image (computed after CS1).
        assert_eq!(fixed.report.cvns, p01_cvns(&fixed.data).unwrap());
        assert_ne!(fixed.report.cvns[1].cvn, before[1].cvn, "EngineCal CVN changes");
        assert_eq!(fixed.report.cvns[2..], before[2..], "other segments unchanged");
        // CVN covers the CS1 word: changing only CS1 changes the CVN.
        let mut cs_only = fixed.data.clone();
        cs_only[0x8001] ^= 1;
        assert_ne!(p01_cvns(&cs_only).unwrap()[1].cvn, fixed.report.cvns[1].cvn);
    }
    #[test]
    fn p01_bad_table_refuses_correction() {
        let mut d = synthetic_p01();
        d[0x514..0x518].copy_from_slice(&0x30000u32.to_be_bytes());
        assert!(correct_checksums(&d).is_err());
        assert!(validate_checksums(&d).unwrap().method_used.contains("report-only"));
        assert!(correct_checksums(&vec![0u8; P01_FULL_IMAGE_SIZE]).is_err(), "zero image has no OS table");
        assert!(correct_checksums(&vec![0u8; CAL_IMAGE_SIZE]).is_err(), "cal-only has no table");
    }
    #[test]
    fn edc16_size_supported() {
        let img = vec![0u8; EDC16_FLASH_SIZE];
        let _ = validate_checksums(&img).unwrap_or_else(|e| panic!("EDC16 size not supported: {}", e));
    }
    #[test]
    fn honda_os_blocks_p01_corrector() {
        let mut img = vec![0u8; P01_FULL_IMAGE_SIZE];
        img[0x100..0x108].copy_from_slice(b"37820-PR");
        let report = validate_checksums(&img).unwrap();
        assert_eq!(report.ecu_family, "HONDA_KEIHIN");
        assert!(report.method_used.contains("blocked"));
        assert!(correct_checksums(&img).is_err());
    }
    #[test]
    fn p59_os_blocks_p01_corrector() {
        let mut img = vec![0u8; P01_FULL_IMAGE_SIZE];
        img[0x100..0x108].copy_from_slice(b"12586243");
        let report = validate_checksums(&img).unwrap();
        assert_eq!(report.ecu_family, "GM_P59");
        assert!(report.method_used.contains("blocked"));
        assert!(correct_checksums(&img).is_err());
    }
}
