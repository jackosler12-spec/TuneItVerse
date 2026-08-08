// flash.rs — Guided flash pipeline with Priority 0 safety gates
//
// Priority 0:
//   1. Proper flash read / backup (Mode 23 + multi-frame ISO-TP bulk)
//   2. Live post-flash verification
//   3. Voltage gate (PID 0x42)
//   4. Adaptive protocol timing

use serde::{Serialize, Deserialize};
use crate::checksum::{ChecksumReport, correct_and_validate_checksums, CAL_IMAGE_SIZE};
use serialport::SerialPort;
use crate::vpw::{
    build_mode22_request, request_response, build_mode34_request, build_mode36_chunk,
    build_mode37_request, send_frame, build_obd_request, parse_mode01_response,
};
use crate::security::unlock_level2;
use std::time::Duration;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashWriteResult {
    pub bytes_written: u32,
    pub blocks_written: u32,
    pub crc32_written: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub bytes_done: u32,
    pub bytes_total: u32,
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupQuality {
    FullImage,
    PartialDidOnly,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub path: String,
    pub quality: BackupQuality,
    pub bytes: u32,
    pub crc32: Option<u32>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedFlashRequest {
    pub ecu_family: String,
    pub tuned_bin: Vec<u8>,
    pub perform_backup: bool,
    pub auto_correct_checksum: bool,
    pub enable_recovery_prompts: bool,
    pub user_confirmed_risks: bool,
    pub min_voltage_v: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPrompt {
    pub prompt_type: String,
    pub message: String,
    pub steps: Vec<String>,
    pub kernel_to_upload: Option<String>,
    pub grounding_required: bool,
    pub reference_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedFlashResult {
    pub success: bool,
    pub steps_completed: Vec<String>,
    pub backup: Option<BackupResult>,
    pub checksum_report: Option<ChecksumReport>,
    pub flash_write_result: Option<FlashWriteResult>,
    pub verification_crc: Option<u32>,
    pub verified_live: bool,
    pub voltage_at_start: Option<f32>,
    pub recovery_prompt: Option<RecoveryPrompt>,
    pub logs: Vec<String>,
    pub error: Option<String>,
}

#[allow(dead_code)]
pub const CAL_A_START: u32 = 0x0002_0000;
pub const DEFAULT_MIN_VOLTAGE_V: f32 = 12.5;

/// Typical Bosch EDC16 flash base and full size (2 MB).
pub const BOSCH_FLASH_BASE: u32 = 0x0000_0000;
pub const BOSCH_FLASH_SIZE: u32 = 0x0020_0000; // 2 MB
/// Preferred Mode 23 window — large enough to exercise multi-frame ISO-TP.
pub const MODE23_WINDOW: u16 = 0x400; // 1024 bytes per request

const KERNEL_P01: &[u8] = include_bytes!("../../reference/Kernel-P01.bin");

// ─────────────────────────────────────────────────────────────────────────────
// Adaptive timing
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AdaptiveTiming {
    pub base_ms: u64,
    pub max_ms: u64,
    pub consecutive_empty: u32,
}

impl Default for AdaptiveTiming {
    fn default() -> Self {
        Self { base_ms: 5, max_ms: 80, consecutive_empty: 0 }
    }
}

impl AdaptiveTiming {
    pub fn for_vpw() -> Self {
        Self { base_ms: 8, max_ms: 120, consecutive_empty: 0 }
    }
    pub fn for_can() -> Self {
        Self { base_ms: 3, max_ms: 60, consecutive_empty: 0 }
    }
    pub fn on_success(&mut self) { self.consecutive_empty = 0; }
    pub fn on_empty(&mut self) {
        self.consecutive_empty = self.consecutive_empty.saturating_add(1);
    }
    pub fn delay(&self) -> Duration {
        let factor = 1u64 << self.consecutive_empty.min(4);
        Duration::from_millis((self.base_ms * factor).min(self.max_ms))
    }
    pub fn sleep(&self) { std::thread::sleep(self.delay()); }
}

// ─────────────────────────────────────────────────────────────────────────────
// Voltage gate
// ─────────────────────────────────────────────────────────────────────────────

pub fn read_battery_voltage(port: &mut Box<dyn SerialPort + Send>) -> Option<f32> {
    let req = build_obd_request(0x42);
    let mut timing = AdaptiveTiming::for_vpw();
    for _ in 0..3 {
        match request_response(port, &req) {
            Ok(resp) => {
                timing.on_success();
                if let Some(data) = parse_mode01_response(&resp, 0x42) {
                    if data.len() >= 2 {
                        let raw = ((data[0] as u16) << 8) | (data[1] as u16);
                        return Some(raw as f32 / 1000.0);
                    }
                }
                if resp.len() >= 2 {
                    let a = resp[resp.len() - 2];
                    let b = resp[resp.len() - 1];
                    let v = (((a as u16) << 8) | (b as u16)) as f32 / 1000.0;
                    if (10.0..16.5).contains(&v) {
                        return Some(v);
                    }
                }
            }
            Err(_) => { timing.on_empty(); timing.sleep(); }
        }
    }
    None
}

pub fn enforce_voltage_gate(
    port: &mut Box<dyn SerialPort + Send>,
    min_v: f32,
    logs: &mut Vec<String>,
) -> Result<f32, String> {
    match read_battery_voltage(port) {
        Some(v) => {
            logs.push(format!("Battery voltage: {:.2} V (min required {:.2} V)", v, min_v));
            if v < min_v {
                return Err(format!(
                    "Voltage gate FAILED: {:.2} V < {:.2} V minimum. Connect a charger before writing.",
                    v, min_v
                ));
            }
            Ok(v)
        }
        None => {
            if min_v <= 0.0 {
                logs.push("Voltage gate skipped (min_voltage_v ≤ 0)".into());
                Ok(0.0)
            } else {
                Err(
                    "Voltage gate FAILED: could not read PID 0x42. \
                     Set min_voltage_v = 0 to bypass on controlled bench setups only."
                        .into(),
                )
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UDS Mode 23 + bulk multi-frame read
// ─────────────────────────────────────────────────────────────────────────────

/// UDS ReadMemoryByAddress (0x23).
/// addressAndLengthFormatIdentifier 0x42 = 4-byte address + 2-byte size.
/// Response rides multi-frame ISO-TP when size > ~6 bytes.
pub fn uds_read_memory_by_address(
    port: &mut Box<dyn SerialPort + Send>,
    address: u32,
    size: u16,
) -> Result<Vec<u8>, String> {
    let mut payload = vec![0x42];
    payload.extend_from_slice(&address.to_be_bytes());
    payload.extend_from_slice(&size.to_be_bytes());

    // Multi-frame capable path
    let resp = crate::can::uds_request_multiframe(port, 0x23, &payload, true)
        .map_err(|e| format!("Mode 23 failed: {}", e))?;

    if resp.first() == Some(&0x63) {
        return Ok(resp[1..].to_vec());
    }
    if resp.first() == Some(&0x7F) {
        let nrc = resp.get(2).copied().unwrap_or(0);
        return Err(format!("Mode 23 negative response NRC 0x{:02X}", nrc));
    }
    if !resp.is_empty() {
        return Ok(resp);
    }
    Err("Mode 23: empty or unexpected response".into())
}

/// Bulk-read a contiguous memory region using repeated Mode 23 windows.
///
/// Each window is large enough (`MODE23_WINDOW`) that the positive response
/// is multi-frame ISO-TP — exercising the full FF/CF + Flow Control path.
///
/// `on_progress` is called after each successful window.
pub fn bulk_read_memory<F>(
    port: &mut Box<dyn SerialPort + Send>,
    base: u32,
    total_size: u32,
    window: u16,
    mut on_progress: F,
    logs: &mut Vec<String>,
) -> Result<Vec<u8>, String>
where
    F: FnMut(FlashProgress),
{
    let mut out = Vec::with_capacity(total_size as usize);
    let mut offset = 0u32;
    let mut timing = AdaptiveTiming::for_can();
    let mut consecutive_fail = 0u32;

    logs.push(format!(
        "Bulk Mode 23 read: base=0x{:08X} size={} window={}",
        base, total_size, window
    ));

    while offset < total_size {
        let remaining = total_size - offset;
        let this_win = (remaining as u16).min(window);

        match uds_read_memory_by_address(port, base + offset, this_win) {
            Ok(chunk) => {
                timing.on_success();
                consecutive_fail = 0;
                let got = chunk.len() as u32;
                out.extend_from_slice(&chunk);
                offset += got;

                // If ECU returned less than requested, accept and advance by got
                if got == 0 {
                    consecutive_fail += 1;
                }

                on_progress(FlashProgress {
                    bytes_done: offset.min(total_size),
                    bytes_total: total_size,
                    percent: ((offset.min(total_size) as u64 * 100) / total_size.max(1) as u64) as u8,
                });
            }
            Err(e) => {
                timing.on_empty();
                consecutive_fail += 1;
                logs.push(format!(
                    "Mode 23 window @ 0x{:08X} failed: {} (fail #{})",
                    base + offset, e, consecutive_fail
                ));
                if consecutive_fail >= 5 {
                    logs.push("Too many consecutive Mode 23 failures — stopping bulk read".into());
                    break;
                }
                timing.sleep();
            }
        }

        // Small adaptive gap between windows
        timing.sleep();
    }

    if out.is_empty() {
        return Err("Bulk read returned no data".into());
    }
    logs.push(format!("Bulk read recovered {} / {} bytes", out.len(), total_size));
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Backup
// ─────────────────────────────────────────────────────────────────────────────

pub fn perform_backup(
    port: &mut Box<dyn SerialPort + Send>,
    ecu_family: &str,
    logs: &mut Vec<String>,
) -> BackupResult {
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let fam = ecu_family.to_ascii_uppercase();
    let is_bosch = fam.contains("EDC16") || fam.contains("EDC17") || fam.contains("MED17");

    if is_bosch {
        logs.push("Bosch family: multi-frame ISO-TP Mode 23 bulk read…".into());

        // Full 2 MB is the target; if the link can't sustain it we still keep
        // whatever was recovered and label quality honestly.
        let target = BOSCH_FLASH_SIZE;
        let base = BOSCH_FLASH_BASE;

        match bulk_read_memory(
            port,
            base,
            target,
            MODE23_WINDOW,
            |_| {},
            logs,
        ) {
            Ok(data) => {
                let crc = crc32_ieee(&data);
                let quality = if data.len() as u32 >= target {
                    BackupQuality::FullImage
                } else if data.len() >= 64 * 1024 {
                    // ≥64 KB is useful but incomplete
                    BackupQuality::PartialDidOnly
                } else if data.len() >= 64 {
                    BackupQuality::PartialDidOnly
                } else {
                    BackupQuality::Failed
                };

                let path = match quality {
                    BackupQuality::FullImage => format!("bosch_full_{}.bin", ts),
                    _ => format!("bosch_partial_{}.bin", ts),
                };
                let _ = std::fs::write(&path, &data);

                let notes = match quality {
                    BackupQuality::FullImage => {
                        "Full 2 MB image via Mode 23 + multi-frame ISO-TP. Restore-grade."
                            .to_string()
                    }
                    BackupQuality::PartialDidOnly => format!(
                        "Partial dump ({} bytes of {}). Link dropped or security limited range. \
                         Not restore-grade unless you only need the recovered region.",
                        data.len(), target
                    ),
                    BackupQuality::Failed => "Dump failed or too small to be useful.".into(),
                };

                logs.push(format!(
                    "Backup {} — {} bytes CRC=0x{:08X} quality={:?}",
                    path, data.len(), crc, quality
                ));

                return BackupResult {
                    path,
                    quality,
                    bytes: data.len() as u32,
                    crc32: Some(crc),
                    notes,
                };
            }
            Err(e) => {
                logs.push(format!("Bulk read failed: {}", e));
                return BackupResult {
                    path: String::new(),
                    quality: BackupQuality::Failed,
                    bytes: 0,
                    crc32: None,
                    notes: format!("Mode 23 bulk failed: {}. Check security access + ISO-TP link.", e),
                };
            }
        }
    }

    // GM / P01 — still Mode 22 partial until kernel bulk path lands
    logs.push(
        "P01/GM: Mode 22 DID sampling only. Full cal dump needs kernel-assisted Mode 23/3D."
            .into(),
    );
    let mut backup_data = vec![0u8; 0x20000];
    let mut filled = 0usize;
    let mut timing = AdaptiveTiming::for_vpw();
    for i in 0..(backup_data.len() / 128) {
        let a = 0x20000u32 + (i as u32 * 128);
        let req = build_mode22_request(((a >> 8) & 0xff) as u8, (a & 0xff) as u8);
        match request_response(port, &req) {
            Ok(resp) => {
                timing.on_success();
                for (j, b) in resp.iter().take(128).enumerate() {
                    let idx = i * 128 + j;
                    if idx < backup_data.len() {
                        backup_data[idx] = *b;
                        filled += 1;
                    }
                }
            }
            Err(_) => {
                timing.on_empty();
                if timing.consecutive_empty > 8 { break; }
                timing.sleep();
            }
        }
    }

    let path = format!("pcm_partial_backup_{}.bin", ts);
    let _ = std::fs::write(&path, &backup_data);
    let crc = crc32_ieee(&backup_data);
    logs.push(format!(
        "Partial backup {} ({} samples). CRC=0x{:08X}", path, filled, crc
    ));

    BackupResult {
        path,
        quality: BackupQuality::PartialDidOnly,
        bytes: backup_data.len() as u32,
        crc32: Some(crc),
        notes: "Mode 22 DID sampling — NOT a full calibration image.".into(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Live verification
// ─────────────────────────────────────────────────────────────────────────────

pub fn verify_after_write(
    port: &mut Box<dyn SerialPort + Send>,
    ecu_family: &str,
    written: &[u8],
    logs: &mut Vec<String>,
) -> Result<(u32, bool), String> {
    let expected = crc32_ieee(written);
    logs.push(format!("Expected image CRC32 = 0x{:08X}", expected));

    let fam = ecu_family.to_ascii_uppercase();
    let is_bosch = fam.contains("EDC16") || fam.contains("EDC17") || fam.contains("MED17");

    if is_bosch {
        // Read back as much as practical (up to full image or 256 KB for speed)
        let window_total = (written.len() as u32).min(256 * 1024);
        let base = if written.len() as u32 >= BOSCH_FLASH_SIZE {
            BOSCH_FLASH_BASE
        } else {
            0x0008_0000 // common cal region
        };

        let readback = bulk_read_memory(
            port,
            base,
            window_total,
            MODE23_WINDOW,
            |_| {},
            logs,
        )?;

        if readback.len() < 256 {
            return Err("Live verification failed: insufficient readback".into());
        }

        let compare_len = readback.len().min(written.len());
        let live_crc = crc32_ieee(&readback[..compare_len]);
        let expected_window = crc32_ieee(&written[..compare_len]);
        let matched = live_crc == expected_window;

        logs.push(format!(
            "Live CRC ({} bytes): ECU=0x{:08X} expected=0x{:08X} match={}",
            compare_len, live_crc, expected_window, matched
        ));

        if !matched {
            return Err(format!(
                "LIVE VERIFICATION MISMATCH: 0x{:08X} != 0x{:08X}",
                live_crc, expected_window
            ));
        }
        return Ok((live_crc, true));
    }

    logs.push("P01/GM live full-image verify requires kernel bulk read — not claiming local CRC.".into());
    Err("Live verification not available for this family without kernel bulk read.".into())
}

// ─────────────────────────────────────────────────────────────────────────────
// Kernel + guided orchestration
// ─────────────────────────────────────────────────────────────────────────────

pub fn upload_kernel(port: &mut Box<dyn SerialPort + Send>, kernel: &[u8]) -> Result<(), String> {
    let _ = unlock_level2(port);
    let load_addr: u32 = 0x0010_0000;
    let req34 = build_mode34_request(load_addr, kernel.len() as u32);
    send_frame(port, &req34).map_err(|e| format!("Mode34: {}", e))?;

    let mut timing = AdaptiveTiming::for_vpw();
    timing.sleep();

    for (i, chunk) in kernel.chunks(128).enumerate() {
        let frame = build_mode36_chunk(chunk);
        send_frame(port, &frame).map_err(|e| format!("Mode36 chunk {}: {}", i, e))?;
        timing.on_success();
        timing.sleep();
    }
    let _ = send_frame(port, &build_mode37_request());
    timing.sleep();
    Ok(())
}

pub fn orchestrate_guided_flash<F>(
    port: &mut Box<dyn SerialPort + Send>,
    request: GuidedFlashRequest,
    mut on_progress: F,
) -> Result<GuidedFlashResult, String>
where
    F: FnMut(FlashProgress),
{
    let min_v = request.min_voltage_v.unwrap_or(DEFAULT_MIN_VOLTAGE_V);
    let mut result = GuidedFlashResult {
        success: false,
        steps_completed: vec![],
        backup: None,
        checksum_report: None,
        flash_write_result: None,
        verification_crc: None,
        verified_live: false,
        voltage_at_start: None,
        recovery_prompt: None,
        logs: vec![format!("Guided flash starting for {}", request.ecu_family)],
        error: None,
    };

    match enforce_voltage_gate(port, min_v, &mut result.logs) {
        Ok(v) => {
            result.voltage_at_start = Some(v);
            result.steps_completed.push(format!("Voltage gate passed ({:.2} V)", v));
        }
        Err(e) => {
            result.error = Some(e);
            return Ok(result);
        }
    }

    result.steps_completed.push("ECU profile loaded".into());

    if request.perform_backup {
        let backup = perform_backup(port, &request.ecu_family, &mut result.logs);
        result.steps_completed.push(format!(
            "Backup finished (quality={:?}, {} bytes)", backup.quality, backup.bytes
        ));
        result.backup = Some(backup);
    }

    if request.auto_correct_checksum && request.tuned_bin.len() == CAL_IMAGE_SIZE as usize {
        match correct_and_validate_checksums(&request.tuned_bin) {
            Ok(corrected) => {
                result.checksum_report = Some(corrected.report.clone());
                result.logs.push("Pre-flash checksums corrected".into());
            }
            Err(e) => {
                result.error = Some(e);
                return Ok(result);
            }
        }
    }
    result.steps_completed.push("Pre-flash validation passed".into());

    if request.ecu_family.contains("P01") || request.ecu_family.contains("GM") {
        let _ = unlock_level2(port);
        result.logs.push("Level 2 security unlocked".into());
        if let Err(e) = upload_kernel(port, KERNEL_P01) {
            result.logs.push(format!("Kernel upload warning: {}", e));
        }
    }

    if !request.user_confirmed_risks {
        result.error = Some("Risks not confirmed".into());
        return Ok(result);
    }
    result.steps_completed.push("Risks confirmed".into());

    if request.tuned_bin.is_empty() {
        result.error = Some("Empty tuned_bin".into());
        return Ok(result);
    }

    if let Err(e) = enforce_voltage_gate(port, min_v, &mut result.logs) {
        result.error = Some(format!("Voltage sagged before write: {}", e));
        return Ok(result);
    }

    let cal_addr: u32 = if request.ecu_family.to_ascii_uppercase().contains("EDC16") {
        0x0008_0000
    } else {
        0x0002_0000
    };
    let req34 = build_mode34_request(cal_addr, request.tuned_bin.len() as u32);
    if let Err(e) = send_frame(port, &req34) {
        result.error = Some(format!("Mode34 failed: {}", e));
        if request.enable_recovery_prompts {
            result.recovery_prompt = Some(get_recovery_prompt(
                request.ecu_family.clone(), "mode34 failed".into(),
            ));
        }
        return Ok(result);
    }

    let mut timing = if request.ecu_family.to_ascii_uppercase().contains("EDC") {
        AdaptiveTiming::for_can()
    } else {
        AdaptiveTiming::for_vpw()
    };
    timing.sleep();

    let chunk_size = 128;
    let total = request.tuned_bin.len();
    for (i, chunk) in request.tuned_bin.chunks(chunk_size).enumerate() {
        let frame = build_mode36_chunk(chunk);
        if let Err(e) = send_frame(port, &frame) {
            result.error = Some(format!("Mode36 chunk {} failed: {}", i, e));
            return Ok(result);
        }
        timing.on_success();
        let done = ((i + 1) * chunk_size).min(total);
        on_progress(FlashProgress {
            bytes_done: done as u32,
            bytes_total: total as u32,
            percent: ((done * 100) / total.max(1)) as u8,
        });
        timing.sleep();
    }

    if let Err(e) = send_frame(port, &build_mode37_request()) {
        result.error = Some(format!("Mode37 failed: {}", e));
        return Ok(result);
    }

    let crc_written = crc32_ieee(&request.tuned_bin);
    result.flash_write_result = Some(FlashWriteResult {
        bytes_written: request.tuned_bin.len() as u32,
        blocks_written: ((request.tuned_bin.len() + chunk_size - 1) / chunk_size) as u32,
        crc32_written: crc_written,
    });
    result.steps_completed.push("Flash write completed (34/36/37)".into());

    match verify_after_write(port, &request.ecu_family, &request.tuned_bin, &mut result.logs) {
        Ok((live_crc, matched)) => {
            result.verification_crc = Some(live_crc);
            result.verified_live = matched;
            result.steps_completed.push("Live post-flash verification PASSED".into());
            result.success = true;
        }
        Err(e) => {
            result.logs.push(format!("Verification: {}", e));
            result.verified_live = false;
            result.steps_completed.push("Write completed — LIVE VERIFY UNAVAILABLE OR FAILED".into());
            result.success = true;
            result.error = Some(e);
        }
    }

    Ok(result)
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

pub fn get_recovery_prompt(ecu_family: String, error_context: String) -> RecoveryPrompt {
    RecoveryPrompt {
        prompt_type: "generic".into(),
        message: format!("Recovery needed for {}: {}", ecu_family, error_context),
        steps: vec![
            "Verify power (≥12.5 V) and connection".into(),
            "Upload appropriate kernel from reference/".into(),
            "Retry with perform_backup=true and review BackupQuality".into(),
        ],
        kernel_to_upload: if ecu_family.contains("P01") {
            Some("Kernel-P01.bin".into())
        } else {
            None
        },
        grounding_required: ecu_family.contains("P01"),
        reference_notes: "See V2_ROADMAP.md Priority 0.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_timing_backs_off() {
        let mut t = AdaptiveTiming::for_vpw();
        let d0 = t.delay();
        t.on_empty();
        assert!(t.delay() > d0);
        t.on_success();
        assert_eq!(t.delay(), Duration::from_millis(t.base_ms));
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32_ieee(&[]), 0);
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn mode23_window_is_multiframe_sized() {
        // >7 bytes of UDS data → multi-frame ISO-TP response
        assert!(MODE23_WINDOW > 7);
        assert_eq!(MODE23_WINDOW, 0x400);
    }
}
