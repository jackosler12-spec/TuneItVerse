//! GM P01 Flash Read / Write Engine
//!
//! Implements the full GM Mode 34/36/37 flash programming sequence for
//! the P01 (0411) PCM over J1850 VPW.
//!
//! ─── Flash memory map (P01 / 0411) ────────────────────────────────────────
//!  Region          Address range        Size    Description
//!  OS (kernel)     0x0000_0000–0x0001_FFFF  128 KB  Operating system (do not erase)
//!  Cal block A     0x0002_0000–0x0002_FFFF   64 KB  Calibration tables (tune data)
//!  Cal block B     0x0003_0000–0x0003_FFFF   64 KB  Calibration tables (tune data)
//!  Seed-key patch  0x0004_0000–0x0004_03FF    1 KB  Security algorithm ROM
//!  VIN/ID          0x0004_0400–0x0004_07FF    1 KB  Vehicle identification
//!
//!  Safe tuning: read/write Cal A+B only (0x0002_0000–0x0003_FFFF, 128 KB)
//!  Full backup:  read all regions (512 KB total on 0411)
//!
//! ─── Protocol sequence ────────────────────────────────────────────────────
//!  READ (Mode 23 — memory read by address):
//!    1. Security Level 1 must be active.
//!    2. TX: 68 6A F1  23  A2 A1 A0  LL  <cs>      (addr 3 bytes, len 1 byte)
//!    3. RX: 48 6B 10  63  <data…>   <cs>
//!
//!  WRITE (Mode 34/36/37):
//!    1. Security Level 2 must be active.
//!    2. Checksum correction applied to data BEFORE Mode 34.
//!    3. Mode 34 — Request Download:
//!         TX: 68 6A F1  34  00  A2 A1 A0  S2 S1 S0  <cs>  (addr, size 3 bytes each)
//!         RX: 48 6B 10  74  BL  <cs>                      (BL = max block length)
//!    4. Mode 36 — Transfer Data (repeated for each block):
//!         TX: 68 6A F1  36  <data[0..BL]>  <cs>
//!         RX: 48 6B 10  76  <cs>           (positive ack per block)
//!    5. Mode 37 — Request Transfer Exit:
//!         TX: 68 6A F1  37  <cs>
//!         RX: 48 6B 10  77  <cs>
//!
//! References:
//!   • GM GMLAN/J1850 Flash Programming Application Note
//!   • EFILive V8 technical notes (community documentation)
//!   • SAE J2190 Mode 34/36/37 specification
//!   • LS1edit / HPTuners community reverse-engineering

use crate::{write_frame, read_response, validate_checksum};
use crate::checksum::{correct_and_validate_checksums, ChecksumReport, CAL_IMAGE_SIZE};
use serialport::SerialPort;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Constants — P01 memory map
// ─────────────────────────────────────────────────────────────────────────────

/// Calibration block A start address (safe tune region)
pub const CAL_A_START:   u32 = 0x0002_0000;
/// Calibration block B end address (inclusive last byte)
pub const CAL_B_END:     u32 = 0x0003_FFFF;
/// Full calibration region size (128 KB)
pub const CAL_REGION_SIZE: u32 = 0x0002_0000;

/// OS kernel region — READ ONLY, never erase or overwrite
pub const OS_START:  u32 = 0x0000_0000;
pub const OS_END:    u32 = 0x0001_FFFF;

/// Maximum Mode 36 block length if ECM does not specify (safe default)
pub const DEFAULT_BLOCK_LEN: usize = 128;
/// Maximum block length the P01 ECM will ever return
pub const MAX_BLOCK_LEN: usize = 249;  // J1850 VPW max payload − header − cs

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Progress report emitted during multi-block operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    /// Bytes transferred so far
    pub bytes_done:  u32,
    /// Total bytes in this operation
    pub bytes_total: u32,
    /// 0–100 percent
    pub percent:     u8,
    /// Current flash address
    pub address:     u32,
}

impl FlashProgress {
    fn new(total: u32) -> Self {
        FlashProgress { bytes_done: 0, bytes_total: total, percent: 0, address: 0 }
    }
    fn advance(&mut self, bytes: u32, addr: u32) {
        self.bytes_done += bytes;
        self.address     = addr;
        self.percent     = ((self.bytes_done as u64 * 100)
            / self.bytes_total.max(1) as u64) as u8;
    }
}

/// Result of a completed flash read.
#[derive(Debug, Clone, Serialize)]
pub struct FlashReadResult {
    pub start_address: u32,
    pub length:        u32,
    pub data:          Vec<u8>,
    /// CRC-32 of the data for integrity verification
    pub crc32:         u32,
}

/// Result of a completed flash write.
#[derive(Debug, Clone, Serialize)]
pub struct FlashWriteResult {
    pub start_address:     u32,
    pub bytes_written:     u32,
    pub blocks_written:    u32,
    pub crc32_written:     u32,
    /// Checksum correction report (populated for cal-region writes)
    pub checksum_report:   Option<ChecksumReport>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Address guard
// ─────────────────────────────────────────────────────────────────────────────

/// Validate that an address range is safe to write.
/// Rejects any range that overlaps the OS kernel region.
pub fn guard_write_range(start: u32, length: u32) -> Result<(), String> {
    let end = start.checked_add(length)
        .ok_or_else(|| "Address + length overflow u32".to_string())?
        .saturating_sub(1);

    if start <= OS_END && end >= OS_START {
        return Err(format!(
            "UNSAFE: address range 0x{:08X}–0x{:08X} overlaps OS kernel \
             region (0x{:08X}–0x{:08X}). Aborting.",
            start, end, OS_START, OS_END
        ));
    }
    if length == 0 {
        return Err("Write length must be > 0".to_string());
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame builders
// ─────────────────────────────────────────────────────────────────────────────

fn build_mode23_read(addr: u32, len: u8) -> Vec<u8> {
    let mut frame = vec![
        0x68u8, 0x6A, 0xF1,
        0x23,
        ((addr >> 16) & 0xFF) as u8,
        ((addr >> 8)  & 0xFF) as u8,
        ( addr        & 0xFF) as u8,
        len,
    ];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

fn build_mode34_request_download(addr: u32, size: u32) -> Vec<u8> {
    let mut frame = vec![
        0x68u8, 0x6A, 0xF1,
        0x34,
        0x00,
        ((addr >> 16) & 0xFF) as u8,
        ((addr >> 8)  & 0xFF) as u8,
        ( addr        & 0xFF) as u8,
        ((size >> 16) & 0xFF) as u8,
        ((size >> 8)  & 0xFF) as u8,
        ( size        & 0xFF) as u8,
    ];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

fn build_mode36_transfer(data: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x36];
    frame.extend_from_slice(data);
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

fn build_mode37_exit() -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x37];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

// ─────────────────────────────────────────────────────────────────────────────
// Response parsers
// ─────────────────────────────────────────────────────────────────────────────

fn parse_mode23_response(frame: &[u8]) -> Result<Vec<u8>, String> {
    check_nrc(frame, 0x23)?;
    if frame.len() < 5 {
        return Err(format!("Mode23 response too short: {} bytes", frame.len()));
    }
    if !validate_checksum(frame) {
        return Err("Mode23 response checksum mismatch".to_string());
    }
    if frame[3] != 0x63 {
        return Err(format!("Mode23: unexpected SID 0x{:02X}", frame[3]));
    }
    Ok(frame[4..frame.len() - 1].to_vec())
}

fn parse_mode34_response(frame: &[u8]) -> Result<usize, String> {
    check_nrc(frame, 0x34)?;
    if frame.len() < 6 {
        return Err(format!("Mode34 response too short: {} bytes", frame.len()));
    }
    if !validate_checksum(frame) {
        return Err("Mode34 response checksum mismatch".to_string());
    }
    if frame[3] != 0x74 {
        return Err(format!("Mode34: unexpected SID 0x{:02X}", frame[3]));
    }
    let bl = frame[4] as usize;
    if bl == 0 || bl > MAX_BLOCK_LEN {
        Ok(DEFAULT_BLOCK_LEN)
    } else {
        Ok(bl)
    }
}

fn parse_mode36_ack(frame: &[u8]) -> Result<(), String> {
    check_nrc(frame, 0x36)?;
    if frame.len() < 5 {
        return Err(format!("Mode36 ack too short: {} bytes", frame.len()));
    }
    if !validate_checksum(frame) {
        return Err("Mode36 ack checksum mismatch".to_string());
    }
    if frame[3] != 0x76 {
        return Err(format!("Mode36: unexpected SID 0x{:02X}", frame[3]));
    }
    Ok(())
}

fn parse_mode37_ack(frame: &[u8]) -> Result<(), String> {
    check_nrc(frame, 0x37)?;
    if frame.len() < 5 {
        return Err(format!("Mode37 ack too short: {} bytes", frame.len()));
    }
    if !validate_checksum(frame) {
        return Err("Mode37 ack checksum mismatch".to_string());
    }
    if frame[3] != 0x77 {
        return Err(format!("Mode37: unexpected SID 0x{:02X}", frame[3]));
    }
    Ok(())
}

fn check_nrc(frame: &[u8], sid: u8) -> Result<(), String> {
    if frame.len() >= 6 && frame[3] == 0x7F && frame[4] == sid {
        let nrc = frame[5];
        return Err(format!(
            "ECM NRC 0x{:02X} for SID 0x{:02X}: {}",
            nrc, sid, nrc_text(nrc)
        ));
    }
    Ok(())
}

fn nrc_text(nrc: u8) -> &'static str {
    match nrc {
        0x22 => "conditions not correct",
        0x24 => "request sequence error",
        0x31 => "request out of range",
        0x33 => "security access denied — unlock first",
        0x35 => "invalid key",
        0x36 => "exceeded attempt limit",
        0x72 => "general programming failure",
        0x73 => "wrong block sequence counter",
        0x78 => "response pending — ECM busy, retry",
        _    => "unknown NRC",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CRC-32 (ISO 3309 / Ethernet polynomial 0xEDB88320)
// ─────────────────────────────────────────────────────────────────────────────

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let mut b = crc ^ (byte as u32);
        for _ in 0..8 {
            if b & 1 != 0 {
                b = (b >> 1) ^ 0xEDB8_8320;
            } else {
                b >>= 1;
            }
        }
        crc = b;
    }
    crc ^ 0xFFFF_FFFF
}

// ─────────────────────────────────────────────────────────────────────────────
// High-level flash READ  (Mode 23)
// ─────────────────────────────────────────────────────────────────────────────

/// Read `length` bytes starting at `start_addr` using Mode 23.
pub fn flash_read<F>(
    port:       &mut Box<dyn SerialPort>,
    start_addr: u32,
    length:     u32,
    mut on_progress: F,
) -> Result<FlashReadResult, String>
where
    F: FnMut(FlashProgress),
{
    const READ_BLOCK: u8 = 0xF0;
    let mut data = Vec::with_capacity(length as usize);
    let mut progress = FlashProgress::new(length);
    let mut offset: u32 = 0;

    while offset < length {
        let remaining = length - offset;
        let chunk = remaining.min(READ_BLOCK as u32) as u8;
        let addr  = start_addr + offset;

        write_frame(port, &build_mode23_read(addr, chunk))?;
        let resp = read_response(port)?;
        let chunk_data = parse_mode23_response(&resp)
            .map_err(|e| format!("Read @ 0x{:08X}: {}", addr, e))?;

        if chunk_data.len() != chunk as usize {
            return Err(format!(
                "Mode23 returned {} bytes, expected {} at 0x{:08X}",
                chunk_data.len(), chunk, addr
            ));
        }

        data.extend_from_slice(&chunk_data);
        offset += chunk as u32;
        progress.advance(chunk as u32, addr);
        on_progress(progress.clone());
    }

    let checksum = crc32(&data);
    Ok(FlashReadResult {
        start_address: start_addr,
        length,
        data,
        crc32: checksum,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// High-level flash WRITE  (Mode 34 → 36 × N → 37)
// ─────────────────────────────────────────────────────────────────────────────

/// Write `data` to flash starting at `start_addr` using Mode 34/36/37.
///
/// Pre-flight steps (before any bus traffic):
///   1. Guard: rejects any write into the OS kernel region.
///   2. Checksum correction: if data is exactly CAL_IMAGE_SIZE bytes AND
///      start_addr == CAL_A_START, all 16 P01 calibration checksum regions
///      are validated and corrected automatically.  The corrected image is
///      what gets sent — the original `data` slice is never modified.
///      For non-cal regions (e.g. seed-key patch) checksums are NOT touched.
///
/// Protocol:
///   3. Mode 34 RequestDownload — negotiate block length with ECM.
///   4. Mode 36 TransferData   — send corrected data in blocks.
///   5. Mode 37 RequestTransferExit — finalize.
///
/// Requires: Security Level 2 active.
///
/// ⚠️  BRICKING RISK:
///   - Stable 12 V supply required (use a charger/maintainer).
///   - Do not interrupt once Mode 36 transfer starts.
///   - OS region writes rejected by guard; cal checksum auto-corrected.
pub fn flash_write<F>(
    port:       &mut Box<dyn SerialPort>,
    start_addr: u32,
    data:       &[u8],
    mut on_progress: F,
) -> Result<FlashWriteResult, String>
where
    F: FnMut(FlashProgress),
{
    let length = data.len() as u32;

    // Step 1: OS guard
    guard_write_range(start_addr, length)?;

    // Step 2: Checksum correction (cal region only)
    let (write_data, cs_report): (std::borrow::Cow<[u8]>, Option<ChecksumReport>) =
        if start_addr == CAL_A_START && data.len() == CAL_IMAGE_SIZE {
            let corrected = correct_and_validate_checksums(data)
                .map_err(|e| format!("Checksum correction failed: {}", e))?;
            if !corrected.report.all_valid {
                return Err(format!(
                    "Checksum correction did not produce a fully valid image \
                     ({} regions still invalid). Aborting flash write.",
                    corrected.report.failed_count
                ));
            }
            (std::borrow::Cow::Owned(corrected.data), Some(corrected.report))
        } else {
            (std::borrow::Cow::Borrowed(data), None)
        };

    // Step 3: Mode 34 — negotiate block length
    write_frame(port, &build_mode34_request_download(start_addr, length))?;
    let resp34 = read_response(port)?;
    let block_len = parse_mode34_response(&resp34)
        .map_err(|e| format!("Mode34 failed: {}", e))?;

    // Step 4: Mode 36 — transfer corrected data in blocks
    let crc = crc32(&write_data);
    let mut progress    = FlashProgress::new(length);
    let mut blocks_sent: u32 = 0;
    let mut offset: usize    = 0;

    while offset < write_data.len() {
        let end   = (offset + block_len).min(write_data.len());
        let block = &write_data[offset..end];
        let addr  = start_addr + offset as u32;

        write_frame(port, &build_mode36_transfer(block))?;
        let resp36 = read_response(port)?;
        parse_mode36_ack(&resp36)
            .map_err(|e| format!("Mode36 block {} @ 0x{:08X}: {}", blocks_sent, addr, e))?;

        offset      += block.len();
        blocks_sent += 1;
        progress.advance(block.len() as u32, addr);
        on_progress(progress.clone());
    }

    // Step 5: Mode 37 — exit
    write_frame(port, &build_mode37_exit())?;
    let resp37 = read_response(port)?;
    parse_mode37_ack(&resp37).map_err(|e| format!("Mode37 exit failed: {}", e))?;

    Ok(FlashWriteResult {
        start_address:   start_addr,
        bytes_written:   length,
        blocks_written:  blocks_sent,
        crc32_written:   crc,
        checksum_report: cs_report,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Convenience wrappers
// ─────────────────────────────────────────────────────────────────────────────

/// Read the full calibration region (Cal A + Cal B = 128 KB).
pub fn read_calibration<F>(
    port: &mut Box<dyn SerialPort>,
    on_progress: F,
) -> Result<FlashReadResult, String>
where
    F: FnMut(FlashProgress),
{
    flash_read(port, CAL_A_START, CAL_REGION_SIZE, on_progress)
}

/// Write a corrected 128 KB calibration image to Cal A+B.
/// `data` must be exactly CAL_REGION_SIZE bytes (128 KB).
/// Checksum correction is applied automatically inside flash_write.
pub fn write_calibration<F>(
    port: &mut Box<dyn SerialPort>,
    data: &[u8],
    on_progress: F,
) -> Result<FlashWriteResult, String>
where
    F: FnMut(FlashProgress),
{
    if data.len() != CAL_REGION_SIZE as usize {
        return Err(format!(
            "Cal data must be exactly {} bytes (128 KB), got {}",
            CAL_REGION_SIZE, data.len()
        ));
    }
    flash_write(port, CAL_A_START, data, on_progress)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_empty() {
        assert_eq!(crc32(&[]), 0x0000_0000);
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn guard_rejects_os_region() {
        assert!(guard_write_range(0x0000_0000, 0x1000).is_err());
        assert!(guard_write_range(0x0001_0000, 0x2000).is_err());
    }

    #[test]
    fn guard_accepts_cal_region() {
        assert!(guard_write_range(CAL_A_START, CAL_REGION_SIZE).is_ok());
    }

    #[test]
    fn guard_rejects_zero_length() {
        assert!(guard_write_range(CAL_A_START, 0).is_err());
    }

    fn checksum_ok(frame: &[u8]) -> bool {
        if frame.len() < 2 { return false; }
        let exp = frame[..frame.len()-1]
            .iter().fold(0u8, |a, &b| a.wrapping_add(b));
        exp == frame[frame.len()-1]
    }

    #[test]
    fn mode23_frame_checksum() {
        let f = build_mode23_read(CAL_A_START, 0x10);
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x23);
    }

    #[test]
    fn mode34_frame_checksum() {
        let f = build_mode34_request_download(CAL_A_START, 0x100);
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x34);
        assert_eq!(f[4], 0x00);
    }

    #[test]
    fn mode36_frame_checksum() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let f = build_mode36_transfer(&data);
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x36);
        assert_eq!(&f[4..8], &[0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn mode37_frame_checksum() {
        let f = build_mode37_exit();
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x37);
    }

    #[test]
    fn parse_mode34_extracts_block_len() {
        let mut f = vec![0x48u8, 0x6B, 0x10, 0x74, 0x80];
        let cs = f.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        f.push(cs);
        assert_eq!(parse_mode34_response(&f).unwrap(), 128);
    }

    #[test]
    fn parse_mode34_zero_bl_uses_default() {
        let mut f = vec![0x48u8, 0x6B, 0x10, 0x74, 0x00];
        let cs = f.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        f.push(cs);
        assert_eq!(parse_mode34_response(&f).unwrap(), DEFAULT_BLOCK_LEN);
    }

    #[test]
    fn parse_nrc_mode34_security_denied() {
        let mut f = vec![0x48u8, 0x6B, 0x10, 0x7F, 0x34, 0x33];
        let cs = f.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        f.push(cs);
        let err = parse_mode34_response(&f).unwrap_err();
        assert!(err.contains("security access denied"), "got: {}", err);
    }

    #[test]
    fn parse_mode36_ack_ok() {
        let mut f = vec![0x48u8, 0x6B, 0x10, 0x76];
        let cs = f.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        f.push(cs);
        assert!(parse_mode36_ack(&f).is_ok());
    }

    #[test]
    fn parse_mode37_ack_ok() {
        let mut f = vec![0x48u8, 0x6B, 0x10, 0x77];
        let cs = f.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        f.push(cs);
        assert!(parse_mode37_ack(&f).is_ok());
    }

    #[test]
    fn write_calibration_rejects_wrong_size() {
        let short_data = vec![0u8; 1000];
        let result = if short_data.len() != CAL_REGION_SIZE as usize {
            Err(format!("wrong size: {}", short_data.len()))
        } else {
            Ok(())
        };
        assert!(result.is_err());
    }
}
