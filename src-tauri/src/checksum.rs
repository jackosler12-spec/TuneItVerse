// checksum.rs — Multi-ECU support: P01 additive + EDC16C41 multipoint CRC32 + additive fallback
//
// v3.9.1: Honda OS on a P01-sized image is report-only. Correction stays fail-closed.

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

pub const P01_REGIONS: [RegionDescriptor; 8] = [
    RegionDescriptor { name: "Main cal", start: 0x0000, end: 0x3FFF, cs_offset: 0x3FFE },
    RegionDescriptor { name: "Fuel tables", start: 0x4000, end: 0x7FFF, cs_offset: 0x7FFE },
    RegionDescriptor { name: "Spark tables", start: 0x8000, end: 0xBFFF, cs_offset: 0xBFFE },
    RegionDescriptor { name: "Idle/misc", start: 0xC000, end: 0xEFFF, cs_offset: 0xEFFE },
    RegionDescriptor { name: "Sensor scaling", start: 0xF000, end: 0xF3FF, cs_offset: 0xF3FE },
    RegionDescriptor { name: "Transmission", start: 0xF400, end: 0xF7FF, cs_offset: 0xF7FE },
    RegionDescriptor { name: "Security patch", start: 0xF800, end: 0xFBFF, cs_offset: 0xFBFE },
    RegionDescriptor { name: "Header/ID", start: 0xFC00, end: 0xFFFF, cs_offset: 0xFFFE },
];

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

fn validate_p01_checksums(data: &[u8]) -> Result<ChecksumReport, String> {
    let blocks = p01_block_count(data.len())?;
    let mut regions = vec![];
    let mut valid_count = 0;
    let mut failed_count = 0;
    for block_idx in 0..blocks {
        let block_offset = block_idx * BLOCK_SIZE;
        let block = &data[block_offset..block_offset + BLOCK_SIZE];
        for region in &P01_REGIONS {
            let cs_abs = block_offset + region.cs_offset;
            let original_cs = read_u16_be(data, cs_abs) as u32;
            let was_valid = validate_additive_region(block, region);
            if was_valid { valid_count += 1; } else { failed_count += 1; }
            regions.push(RegionResult {
                name: region.name.to_string(), block: block_idx as u8, cs_offset: cs_abs,
                original_cs, corrected_cs: original_cs, was_valid, is_valid: was_valid,
                method: "Additive16".to_string(),
            });
        }
    }
    Ok(ChecksumReport { regions, valid_count, fixed_count: 0, failed_count, all_valid: failed_count == 0, ecu_family: "P01_0411".to_string(), method_used: "Additive16".to_string() })
}

fn correct_p01_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    let blocks = p01_block_count(data.len())?;
    let mut buf = data.to_vec();
    let mut results = vec![];
    let mut valid_count = 0;
    let mut fixed_count = 0;
    let mut failed_count = 0;
    for block_idx in 0..blocks {
        let block_offset = block_idx * BLOCK_SIZE;
        for region in &P01_REGIONS {
            let cs_abs = block_offset + region.cs_offset;
            let original_cs = read_u16_be(&buf, cs_abs) as u32;
            let block_slice = &buf[block_offset..block_offset + BLOCK_SIZE];
            let was_valid = validate_additive_region(block_slice, region);
            let (corrected_cs, is_valid) = if was_valid {
                valid_count += 1; (original_cs, true)
            } else {
                let block_mut = &mut buf[block_offset..block_offset + BLOCK_SIZE];
                match correct_additive_region(block_mut, region) {
                    Ok(new_cs) => { fixed_count += 1; (new_cs as u32, true) }
                    Err(e) => { failed_count += 1; return Err(format!("Block {} region '{}': {}", block_idx, region.name, e)); }
                }
            };
            results.push(RegionResult {
                name: region.name.to_string(), block: block_idx as u8, cs_offset: cs_abs,
                original_cs, corrected_cs, was_valid, is_valid, method: "Additive16".to_string(),
            });
        }
    }
    Ok(CorrectedCal { data: buf, report: ChecksumReport { regions: results, valid_count, fixed_count, failed_count, all_valid: failed_count == 0, ecu_family: "P01_0411".to_string(), method_used: "Additive16".to_string() } })
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
    Ok(ChecksumReport { regions, valid_count, fixed_count: 0, failed_count, all_valid: failed_count == 0, ecu_family: "EDC16C41".to_string(), method_used: "CRC32 multipoint".to_string() })
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
    Ok(CorrectedCal { data: buf, report: ChecksumReport { regions: results, valid_count, fixed_count, failed_count, all_valid: failed_count == 0, ecu_family: "EDC16C41".to_string(), method_used: "CRC32 multipoint".to_string() } })
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
            method_used: "report-only: Honda OS on P01-sized image — P01 additive blocked".into(),
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
        })
    }
}

pub fn correct_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    if crate::cs_guard::honda_blocks_p01_corrector(data) {
        return Err("Honda OS string on this image. P01 additive correction is blocked.".into());
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
                ecu_family: pre.ecu_family, method_used: pre.method_used,
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
mod tests {
    use super::*;
    fn zero_image_p01() -> Vec<u8> { vec![0u8; CAL_IMAGE_SIZE] }
    #[test]
    fn validate_unknown_size_is_report_only() {
        let r = validate_checksums(&vec![0u8; 100]).expect("unknown size must report, not error");
        assert_eq!(r.ecu_family, "UNKNOWN");
        assert!(r.method_used.contains("report-only"));
        assert!(correct_checksums(&vec![0u8; 100]).is_err());
    }
    #[test]
    fn p01_512kb_is_supported() {
        let img = vec![0u8; P01_FULL_IMAGE_SIZE];
        let report = validate_checksums(&img).expect("512KB P01 must be accepted");
        assert_eq!(report.regions.len(), 64);
        assert!(report.all_valid);
        let mut dirty = img;
        dirty[0x3FFE] = 0x12; dirty[0x3FFF] = 0x34;
        let fixed = correct_checksums(&dirty).unwrap();
        assert!(fixed.report.all_valid);
        assert!(fixed.report.fixed_count >= 1);
        assert_eq!(fixed.data.len(), P01_FULL_IMAGE_SIZE);
    }
    #[test]
    fn correct_makes_valid_p01() {
        let img = zero_image_p01();
        let corrected = correct_checksums(&img).unwrap();
        assert!(corrected.report.all_valid);
        assert_eq!(corrected.report.regions.len(), 16);
        let mut dirty = img;
        dirty[0x3FFE] = 0x12; dirty[0x3FFF] = 0x34;
        let fixed = correct_checksums(&dirty).unwrap();
        assert!(fixed.report.all_valid);
        assert!(fixed.report.fixed_count >= 1);
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
}
