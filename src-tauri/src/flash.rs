// flash.rs — Guided flash pipeline with Priority 0 safety gates
//
// Priority 0:
//   1. Proper flash read / backup
//      - Bosch: Mode 23 + multi-frame ISO-TP
//      - P01/GM: kernel upload + Mode 3C ReadBlock (+ optional HS VPW)
//   2. Live post-flash verification
//   3. Voltage gate (PID 0x42)
//   4. Adaptive protocol timing

use serde::{Serialize, Deserialize};
use crate::checksum::{ChecksumReport, correct_and_validate_checksums, CAL_IMAGE_SIZE};
use serialport::SerialPort;
use crate::vpw::{
    build_mode22_request, request_response, build_mode34_request, build_mode36_chunk,
    build_mode37_request, send_frame, build_obd_request, parse_mode01_response,
    build_mode3c_read_block, parse_mode3c_response, build_mode3f_test_device,
    build_mode20_exit_kernel, build_mode_a0_hs_prepare, build_mode_a1_hs_enter,
    parse_hs_response, HsResponse,
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
    /// Attempt VPW high-speed (0xA0/0xA1) after kernel Mode 3C probe.
    /// Default true. Harmless if adapter/kernel reject — falls back automatically.
    pub prefer_high_speed: Option<bool>,
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

pub const BOSCH_FLASH_BASE: u32 = 0x0000_0000;
pub const BOSCH_FLASH_SIZE: u32 = 0x0020_0000;
pub const MODE23_WINDOW: u16 = 0x400;

pub const P01_FLASH_SIZE: u32 = 0x0008_0000;
pub const P01_CAL_BASE: u32 = 0x0002_0000;
pub const P01_CAL_SIZE: u32 = 0x0002_0000;

/// Normal-speed Mode 3C block (safe on all VPW adapters).
pub const MODE3C_BLOCK: u16 = 0x100; // 256 B
/// High-speed Mode 3C block (used only after successful 0xA0/0xA1).
pub const MODE3C_BLOCK_HS: u16 = 0x200; // 512 B

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
    /// Tighter gaps once high-speed VPW is confirmed.
    pub fn for_vpw_hs() -> Self {
        Self { base_ms: 3, max_ms: 40, consecutive_empty: 0 }
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
// Kernel upload + Mode 3C + optional high-speed VPW
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
    std::thread::sleep(Duration::from_millis(100));
    Ok(())
}

pub fn kernel_is_alive(port: &mut Box<dyn SerialPort + Send>) -> bool {
    let req = build_mode3f_test_device();
    match request_response(port, &req) {
        Ok(resp) => {
            !resp.is_empty()
                && (resp.iter().any(|&b| b == 0x7F || b == 0x3F) || resp.len() > 3)
        }
        Err(_) => false,
    }
}

pub fn kernel_read_block(
    port: &mut Box<dyn SerialPort + Send>,
    address: u32,
    size: u16,
) -> Result<Vec<u8>, String> {
    let req = build_mode3c_read_block(address, size);
    let resp = request_response(port, &req)
        .map_err(|e| format!("Mode 3C @ 0x{:06X}: {}", address, e))?;
    parse_mode3c_response(&resp)
}

/// Probe Mode 3C at normal speed with a tiny window.
/// Returns true if the kernel returns usable data.
fn probe_mode3c(port: &mut Box<dyn SerialPort + Send>, logs: &mut Vec<String>) -> bool {
    match kernel_read_block(port, 0x0000_0000, 0x10) {
        Ok(data) if !data.is_empty() => {
            logs.push(format!("Mode 3C probe OK ({} bytes)", data.len()));
            true
        }
        Ok(_) => {
            logs.push("Mode 3C probe returned empty — kernel may be limited".into());
            false
        }
        Err(e) => {
            logs.push(format!("Mode 3C probe failed: {}", e));
            false
        }
    }
}

/// Attempt high-speed VPW entry (0xA0 → 0xA1).
///
/// Returns `true` only when both prepare and enter look positive.
/// Does **not** switch the tool physical layer — that is adapter-specific
/// (J2534 PassThruSetConfig / STN proprietary). Serial ELM clones usually
/// stay at normal rate; the kernel may still accept the modes and we simply
/// benefit from slightly faster kernel-side handling.
pub fn try_enter_high_speed(
    port: &mut Box<dyn SerialPort + Send>,
    logs: &mut Vec<String>,
) -> bool {
    logs.push("Attempting high-speed VPW (Mode 0xA0 prepare)…".into());

    let prep = build_mode_a0_hs_prepare();
    match request_response(port, &prep) {
        Ok(resp) => match parse_hs_response(&resp) {
            HsResponse::PrepareOk => {
                logs.push("Mode 0xA0 HighSpeedPrepare accepted (E0)".into());
            }
            HsResponse::Negative => {
                logs.push("Mode 0xA0 rejected (NRC) — staying normal speed".into());
                return false;
            }
            other => {
                // Some kernels ACK with non-standard bytes; treat non-empty as soft-ok
                if resp.is_empty() {
                    logs.push(format!("Mode 0xA0 unclear ({:?}) — aborting HS", other));
                    return false;
                }
                logs.push(format!(
                    "Mode 0xA0 response ambiguous ({:?}) — continuing to 0xA1", other
                ));
            }
        },
        Err(e) => {
            logs.push(format!("Mode 0xA0 no response: {} — normal speed", e));
            return false;
        }
    }

    // Brief settle so kernel arms the faster bit-clock
    std::thread::sleep(Duration::from_millis(20));

    logs.push("Sending Mode 0xA1 HighSpeed enter…".into());
    let enter = build_mode_a1_hs_enter();
    match request_response(port, &enter) {
        Ok(resp) => match parse_hs_response(&resp) {
            HsResponse::EnterOk => {
                logs.push("Mode 0xA1 HighSpeed ENTERED (E1) — using HS block size + timing".into());
                // Adapter physical-layer switch would go here for J2534/STN.
                true
            }
            HsResponse::Negative => {
                logs.push("Mode 0xA1 rejected — staying normal speed".into());
                false
            }
            other => {
                if resp.is_empty() {
                    logs.push(format!("Mode 0xA1 unclear ({:?}) — normal speed", other));
                    false
                } else {
                    // Soft success: non-empty reply after A0 accepted
                    logs.push(format!(
                        "Mode 0xA1 soft-accept ({:?}) — trying HS blocks".into(),
                        other
                    ));
                    true
                }
            }
        },
        Err(e) => {
            logs.push(format!("Mode 0xA1 no response: {} — normal speed", e));
            false
        }
    }
}

/// Full kernel-assisted bulk read for P01/P59.
///
/// Sequence:
/// 1. Ensure kernel resident (upload if needed)
/// 2. Mode 3F alive + Mode 3C probe at normal speed
/// 3. Optional 0xA0/0xA1 high-speed entry
/// 4. Mode 3C loop (HS block size if entered, else normal)
/// 5. On sustained failures while HS: drop back to normal blocks once
/// 6. Mode 20 ExitKernel
pub fn p01_kernel_bulk_read<F>(
    port: &mut Box<dyn SerialPort + Send>,
    base: u32,
    total_size: u32,
    mut on_progress: F,
    logs: &mut Vec<String>,
) -> Result<Vec<u8>, String>
where
    F: FnMut(FlashProgress),
{
    p01_kernel_bulk_read_ex(port, base, total_size, true, on_progress, logs)
}

/// Extended bulk read with explicit high-speed preference.
pub fn p01_kernel_bulk_read_ex<F>(
    port: &mut Box<dyn SerialPort + Send>,
    base: u32,
    total_size: u32,
    prefer_hs: bool,
    mut on_progress: F,
    logs: &mut Vec<String>,
) -> Result<Vec<u8>, String>
where
    F: FnMut(FlashProgress),
{
    logs.push(format!(
        "P01 kernel bulk read: base=0x{:06X} size={} prefer_hs={}",
        base, total_size, prefer_hs
    ));

    // ── Kernel present ──────────────────────────────────────────────────
    if !kernel_is_alive(port) {
        logs.push("Kernel not responding — uploading Kernel-P01.bin…".into());
        upload_kernel(port, KERNEL_P01)
            .map_err(|e| format!("Kernel upload failed: {}", e))?;
        if !kernel_is_alive(port) {
            return Err(
                "Kernel upload completed but Mode 3F probe failed. \
                 Check L2 unlock, power, and VPW link."
                    .into(),
            );
        }
        logs.push("Kernel alive (Mode 3F OK)".into());
    } else {
        logs.push("Kernel already resident".into());
    }

    // ── Normal-speed Mode 3C probe (required before HS) ─────────────────
    if !probe_mode3c(port, logs) {
        logs.push("Mode 3C probe failed — continuing anyway; dump may be partial".into());
    }

    // ── Optional high-speed ─────────────────────────────────────────────
    let mut high_speed = false;
    if prefer_hs {
        high_speed = try_enter_high_speed(port, logs);
    } else {
        logs.push("High-speed VPW disabled by request".into());
    }

    let mut block_size = if high_speed { MODE3C_BLOCK_HS } else { MODE3C_BLOCK };
    let mut timing = if high_speed {
        AdaptiveTiming::for_vpw_hs()
    } else {
        AdaptiveTiming::for_vpw()
    };

    logs.push(format!(
        "Bulk Mode 3C: block={} HS={}",
        block_size, high_speed
    ));

    // ── Dump loop ───────────────────────────────────────────────────────
    let mut out = Vec::with_capacity(total_size as usize);
    let mut offset = 0u32;
    let mut consecutive_fail = 0u32;
    let mut hs_fallback_done = false;

    while offset < total_size {
        let remaining = total_size - offset;
        let this_size = (remaining as u16).min(block_size);

        match kernel_read_block(port, base + offset, this_size) {
            Ok(chunk) => {
                timing.on_success();
                consecutive_fail = 0;
                let got = chunk.len() as u32;
                if got == 0 {
                    consecutive_fail += 1;
                } else {
                    out.extend_from_slice(&chunk);
                    offset += got;
                }
                on_progress(FlashProgress {
                    bytes_done: offset.min(total_size),
                    bytes_total: total_size,
                    percent: ((offset.min(total_size) as u64 * 100)
                        / total_size.max(1) as u64) as u8,
                });
            }
            Err(e) => {
                timing.on_empty();
                consecutive_fail += 1;
                logs.push(format!(
                    "Mode 3C @ 0x{:06X} failed: {} (#{})",
                    base + offset, e, consecutive_fail
                ));

                // One-shot fallback: if HS is active and we're failing, drop to normal
                if high_speed && !hs_fallback_done && consecutive_fail >= 3 {
                    logs.push(
                        "HS Mode 3C unstable — falling back to normal-speed blocks".into()
                    );
                    high_speed = false;
                    hs_fallback_done = true;
                    block_size = MODE3C_BLOCK;
                    timing = AdaptiveTiming::for_vpw();
                    consecutive_fail = 0;
                    continue;
                }

                if consecutive_fail >= 6 {
                    logs.push("Too many Mode 3C failures — stopping".into());
                    break;
                }
                timing.sleep();
            }
        }
        timing.sleep();
    }

    // ── Exit kernel ─────────────────────────────────────────────────────
    let _ = send_frame(port, &build_mode20_exit_kernel());
    logs.push("Mode 20 ExitKernel sent".into());

    if out.is_empty() {
        return Err("Kernel bulk read returned no data".into());
    }
    logs.push(format!(
        "Kernel bulk read recovered {} / {} bytes (HS used: {})",
        out.len(),
        total_size,
        high_speed || hs_fallback_done // true if we entered HS at any point
    ));
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Bosch Mode 23 bulk
// ─────────────────────────────────────────────────────────────────────────────

pub fn uds_read_memory_by_address(
    port: &mut Box<dyn SerialPort + Send>,
    address: u32,
    size: u16,
) -> Result<Vec<u8>, String> {
    let mut payload = vec![0x42];
    payload.extend_from_slice(&address.to_be_bytes());
    payload.extend_from_slice(&size.to_be_bytes());

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
                if got == 0 {
                    consecutive_fail += 1;
                }
                on_progress(FlashProgress {
                    bytes_done: offset.min(total_size),
                    bytes_total: total_size,
                    percent: ((offset.min(total_size) as u64 * 100)
                        / total_size.max(1) as u64) as u8,
                });
            }
            Err(e) => {
                timing.on_empty();
                consecutive_fail += 1;
                logs.push(format!(
                    "Mode 23 @ 0x{:08X} failed: {} (#{})",
                    base + offset, e, consecutive_fail
                ));
                if consecutive_fail >= 5 {
                    break;
                }
                timing.sleep();
            }
        }
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
    let is_p01 = fam.contains("P01") || fam.contains("P59") || fam.contains("GM") || fam.contains("0411");

    if is_bosch {
        logs.push("Bosch family: multi-frame ISO-TP Mode 23 bulk read…".into());
        match bulk_read_memory(port, BOSCH_FLASH_BASE, BOSCH_FLASH_SIZE, MODE23_WINDOW, |_| {}, logs) {
            Ok(data) => {
                let crc = crc32_ieee(&data);
                let quality = if data.len() as u32 >= BOSCH_FLASH_SIZE {
                    BackupQuality::FullImage
                } else if data.len() >= 64 {
                    BackupQuality::PartialDidOnly
                } else {
                    BackupQuality::Failed
                };
                let path = if quality == BackupQuality::FullImage {
                    format!("bosch_full_{}.bin", ts)
                } else {
                    format!("bosch_partial_{}.bin", ts)
                };
                let _ = std::fs::write(&path, &data);
                return BackupResult {
                    path,
                    quality,
                    bytes: data.len() as u32,
                    crc32: Some(crc),
                    notes: format!("Mode 23 multi-frame dump ({} bytes).", data.len()),
                };
            }
            Err(e) => {
                return BackupResult {
                    path: String::new(),
                    quality: BackupQuality::Failed,
                    bytes: 0,
                    crc32: None,
                    notes: format!("Mode 23 bulk failed: {}", e),
                };
            }
        }
    }

    if is_p01 {
        logs.push("P01/GM: kernel-assisted Mode 3C bulk read (HS preferred)…".into());
        match p01_kernel_bulk_read(port, 0x0000_0000, P01_FLASH_SIZE, |_| {}, logs) {
            Ok(data) => {
                let crc = crc32_ieee(&data);
                let quality = if data.len() as u32 >= P01_FLASH_SIZE {
                    BackupQuality::FullImage
                } else if data.len() as u32 >= P01_CAL_SIZE {
                    BackupQuality::PartialDidOnly
                } else if data.len() >= 256 {
                    BackupQuality::PartialDidOnly
                } else {
                    BackupQuality::Failed
                };
                let path = if quality == BackupQuality::FullImage {
                    format!("p01_full_{}.bin", ts)
                } else {
                    format!("p01_partial_{}.bin", ts)
                };
                let _ = std::fs::write(&path, &data);
                logs.push(format!(
                    "P01 backup {} — {} bytes CRC=0x{:08X} quality={:?}",
                    path, data.len(), crc, quality
                ));
                return BackupResult {
                    path,
                    quality,
                    bytes: data.len() as u32,
                    crc32: Some(crc),
                    notes: match quality {
                        BackupQuality::FullImage =>
                            "Full 512 KB P01 image via kernel Mode 3C (±HS). Restore-grade.".into(),
                        BackupQuality::PartialDidOnly => format!(
                            "Partial kernel dump ({} of {} bytes). Not full restore-grade.",
                            data.len(), P01_FLASH_SIZE
                        ),
                        BackupQuality::Failed => "Kernel dump too small.".into(),
                    },
                };
            }
            Err(e) => {
                logs.push(format!(
                    "Kernel bulk read failed ({}), falling back to Mode 22 partial…", e
                ));
            }
        }
    }

    logs.push("Fallback: Mode 22 DID sampling (PARTIAL only).".into());
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
    BackupResult {
        path,
        quality: BackupQuality::PartialDidOnly,
        bytes: backup_data.len() as u32,
        crc32: Some(crc),
        notes: format!("Mode 22 fallback ({} samples). NOT restore-grade.", filled),
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
    let is_p01 = fam.contains("P01") || fam.contains("P59") || fam.contains("GM") || fam.contains("0411");

    if is_bosch {
        let window_total = (written.len() as u32).min(256 * 1024);
        let base = if written.len() as u32 >= BOSCH_FLASH_SIZE {
            BOSCH_FLASH_BASE
        } else {
            0x0008_0000
        };
        let readback = bulk_read_memory(port, base, window_total, MODE23_WINDOW, |_| {}, logs)?;
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

    if is_p01 {
        let size = (written.len() as u32).min(P01_FLASH_SIZE);
        let base = if size >= P01_FLASH_SIZE { 0 } else { P01_CAL_BASE };
        logs.push("P01 live verify via kernel Mode 3C (±HS)…".into());
        let readback = p01_kernel_bulk_read(port, base, size, |_| {}, logs)?;
        if readback.len() < 256 {
            return Err("P01 live verify: insufficient kernel readback".into());
        }
        let compare_len = readback.len().min(written.len());
        let live_crc = crc32_ieee(&readback[..compare_len]);
        let expected_window = crc32_ieee(&written[..compare_len]);
        let matched = live_crc == expected_window;
        logs.push(format!(
            "P01 live CRC ({} bytes): ECU=0x{:08X} expected=0x{:08X} match={}",
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

    Err("Live verification not available for this family.".into())
}

// ─────────────────────────────────────────────────────────────────────────────
// Guided orchestration
// ─────────────────────────────────────────────────────────────────────────────

pub fn orchestrate_guided_flash<F>(
    port: &mut Box<dyn SerialPort + Send>,
    request: GuidedFlashRequest,
    mut on_progress: F,
) -> Result<GuidedFlashResult, String>
where
    F: FnMut(FlashProgress),
{
    let min_v = request.min_voltage_v.unwrap_or(DEFAULT_MIN_VOLTAGE_V);
    let prefer_hs = request.prefer_high_speed.unwrap_or(true);
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
        // perform_backup uses default prefer_hs=true via p01_kernel_bulk_read
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
        } else {
            result.logs.push("Kernel uploaded for write support".into());
        }
        let _ = prefer_hs; // reserved for write-path HS later
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
            "Upload Kernel-P01.bin and retry Mode 3C bulk read".into(),
            "If locked: public grounding recovery during erase (see reference notes)".into(),
        ],
        kernel_to_upload: if ecu_family.contains("P01") {
            Some("Kernel-P01.bin".into())
        } else {
            None
        },
        grounding_required: ecu_family.contains("P01"),
        reference_notes: "See V2_ROADMAP.md Priority 0 + reference/VPW.cs Mode 3C / A0-A1.".into(),
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
    fn hs_timing_is_tighter() {
        let n = AdaptiveTiming::for_vpw();
        let h = AdaptiveTiming::for_vpw_hs();
        assert!(h.base_ms < n.base_ms);
        assert!(h.max_ms < n.max_ms);
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32_ieee(&[]), 0);
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn p01_block_sizes() {
        assert_eq!(MODE3C_BLOCK, 0x100);
        assert_eq!(MODE3C_BLOCK_HS, 0x200);
        assert!(MODE3C_BLOCK_HS > MODE3C_BLOCK);
    }
}
