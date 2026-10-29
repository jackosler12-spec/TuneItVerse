// medc17_checksum.rs — Port of open-source MEDC17 checksum detection/correction (MIT, ConnorHowell/medc17-checksum-tool)
// Supports CRC32 / ADD32 / ADD16 block detection via FADECAFE/CAFEAFFE magic for Bosch MED17/EDC17.
// Experimental; verify on personal dumps. Not hardware-tested.

use serde::{Serialize, Deserialize};

const MAGIC_A: u32 = 0xFADECAFE;
const MAGIC_B: u32 = 0xCAFEAFFE;
const MAGIC_PAIR: [u8; 8] = [0xFE, 0xCA, 0xDE, 0xFA, 0xFF, 0xEA, 0xFE, 0xCA];
const MAGIC_OFFSET: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Medc17Block {
    pub offset: usize,
    pub identifier: u8,
    pub name: String,
    pub structures: Vec<Medc17CsStruct>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Medc17CsStruct {
    pub cs_start: usize,
    pub cs_end: usize,
    pub algorithm: String,
    pub expected: u32,
    pub current: u32,
    pub valid: bool,
}

fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for i in 0..256 {
        let mut crc = i as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
        table[i] = crc;
    }
    table
}

fn calculate_crc32(data: &[u8], start: usize, end_incl: usize, init: u32) -> u32 {
    let table = crc32_table();
    let mut crc = init;
    for i in start..=end_incl {
        if i >= data.len() { break; }
        crc = (crc >> 8) ^ table[((data[i] as u32 ^ crc) & 0xFF) as usize];
    }
    crc
}

fn calculate_add32(data: &[u8], start: usize, end_incl: usize, init: u32) -> u32 {
    let mut sum = init;
    let mut pos = start;
    while pos + 3 <= end_incl && pos + 3 < data.len() {
        let dword = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
        sum = sum.wrapping_add(dword);
        pos += 4;
    }
    sum
}

pub fn detect_medc17_blocks(data: &[u8]) -> Vec<Medc17Block> {
    let mut blocks = Vec::new();
    let mut search = 0;
    while search + 8 < data.len() {
        if let Some(rel) = data[search..].windows(8).position(|w| w == MAGIC_PAIR) {
            let pos = search + rel;
            let hdr = if pos >= MAGIC_OFFSET { pos - MAGIC_OFFSET } else { 0 };
            if is_valid_header(data, hdr) {
                if let Some(block) = parse_block(data, hdr) {
                    blocks.push(block);
                }
            }
            search = pos + 1;
        } else {
            break;
        }
    }
    blocks
}

fn is_valid_header(data: &[u8], hdr: usize) -> bool {
    hdr + 72 < data.len() && data[hdr] != 0 && data[hdr+1] == 0 && data[hdr+3] == 0
}

fn parse_block(data: &[u8], hdr: usize) -> Option<Medc17Block> {
    let identifier = *data.get(hdr)?;
    let name = match identifier {
        0x10 => "Startup",
        0x30 => "Customer",
        0x40 => "Application",
        0x60 => "Dataset",
        0xC0 => "Absolute constants",
        _ => "Unknown",
    }.to_string();
    let mut structures = Vec::new();
    let mut off = hdr + 0x34;
    for _ in 0..8 {
        if off + 32 > data.len() { break; }
        let algo = data[off + 28];
        let cs_start = u32::from_le_bytes(data[off+4..off+8].try_into().ok()?) as usize;
        let cs_end = u32::from_le_bytes(data[off+8..off+12].try_into().ok()?) as usize;
        let expected = u32::from_le_bytes(data[off+16..off+20].try_into().ok()?);
        let algo_name = match algo & 0xFF {
            0x00 => "CRC32",
            0x01 => "ADD32",
            0x10 => "ADD16",
            _ => "UNKNOWN",
        }.to_string();
        let current = if algo_name == "CRC32" {
            calculate_crc32(data, cs_start, cs_end, MAGIC_A)
        } else if algo_name == "ADD32" {
            calculate_add32(data, cs_start, cs_end, MAGIC_A)
        } else {
            0
        };
        structures.push(Medc17CsStruct {
            cs_start,
            cs_end,
            algorithm: algo_name,
            expected,
            current,
            valid: current == expected,
        });
        off += 32;
    }
    Some(Medc17Block { offset: hdr, identifier, name, structures })
}

pub fn report_medc17_checksums(data: &[u8]) -> String {
    let blocks = detect_medc17_blocks(data);
    if blocks.is_empty() {
        return "No MEDC17-style blocks detected (no FADECAFE/CAFEAFFE magic).".to_string();
    }
    let mut report = format!("Detected {} MEDC17 blocks:\n", blocks.len());
    for b in &blocks {
        report.push_str(&format!("  Block @0x{:X} ({})\n", b.offset, b.name));
        for s in &b.structures {
            report.push_str(&format!("    {} 0x{:X}-0x{:X} expected 0x{:08X} current 0x{:08X} {}\n",
                s.algorithm, s.cs_start, s.cs_end, s.expected, s.current,
                if s.valid { "VALID" } else { "INVALID" }));
        }
    }
    report
}

pub fn correct_medc17_add32(data: &mut [u8]) -> Result<usize, String> {
    let blocks = detect_medc17_blocks(data);
    let mut fixed = 0;
    for b in blocks {
        for s in b.structures {
            if s.algorithm == "ADD32" && !s.valid && s.cs_end + 4 <= data.len() {
                let computed = calculate_add32(data, s.cs_start, s.cs_end.saturating_sub(4), MAGIC_A);
                let needed = MAGIC_B.wrapping_sub(computed);
                let patch_off = s.cs_end.saturating_sub(3);
                if patch_off + 4 <= data.len() {
                    data[patch_off..patch_off+4].copy_from_slice(&needed.to_le_bytes());
                    fixed += 1;
                }
            }
        }
    }
    Ok(fixed)
}
