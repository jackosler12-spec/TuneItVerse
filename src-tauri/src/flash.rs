// flash.rs -- Guided flash pipeline v3.35.0 (J2534 mid-write supply watch, no PID during transfer)
use serde::{Serialize, Deserialize};
use crate::checksum::ChecksumReport;
use serialport::SerialPort;
use crate::vpw::{request_response, build_mode34_request, build_mode36_chunk, build_mode37_request, send_frame, build_obd_request, parse_mode01_response};
use crate::security::unlock_level2;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashWriteResult { pub bytes_written: u32, pub blocks_written: u32, pub crc32_written: u32 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub bytes_done: u32,
    pub bytes_total: u32,
    pub percent: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage_warn: Option<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupQuality { FullImage, PartialDidOnly, Failed }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult { pub path: String, pub quality: BackupQuality, pub bytes: u32, pub crc32: Option<u32>, pub notes: String }
fn default_true() -> bool { true }
fn default_false() -> bool { false }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedFlashRequest {
    pub ecu_family: String,
    #[serde(alias = "bin_bytes", alias = "tuned_bin", default)] pub tuned_bin: Vec<u8>,
    #[serde(alias = "do_backup", alias = "perform_backup", default = "default_true")] pub perform_backup: bool,
    #[serde(alias = "auto_correct", alias = "auto_correct_checksum", default = "default_true")] pub auto_correct_checksum: bool,
    #[serde(default = "default_true")] pub enable_recovery_prompts: bool,
    #[serde(alias = "risks", alias = "user_confirmed_risks", default = "default_false")] pub user_confirmed_risks: bool,
    pub min_voltage_v: Option<f32>,
    pub prefer_high_speed: Option<bool>,
    #[serde(default = "default_false")] pub accept_unverified_write: bool,
    /// P01: path of a saved p01_backup image (with .json sidecar) from p01_full_backup.
    #[serde(default)] pub backup_path: Option<String>,
    /// P01 kernel write is experimental and off by default. Requires typed UI confirmation.
    #[serde(default = "default_false")] pub experimental_p01_write: bool,
    /// Must equal EXPERIMENTAL_CONFIRM_PHRASE when experimental_p01_write is set.
    #[serde(default)] pub experimental_confirm: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPrompt { pub prompt_type: String, pub message: String, pub steps: Vec<String>, pub kernel_to_upload: Option<String>, pub grounding_required: bool, pub reference_notes: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedFlashResult {
    pub success: bool, pub steps_completed: Vec<String>, pub backup: Option<BackupResult>,
    pub checksum_report: Option<ChecksumReport>, pub flash_write_result: Option<FlashWriteResult>,
    pub verification_crc: Option<u32>, pub verified_live: bool, pub voltage_at_start: Option<f32>,
    #[serde(default)] pub voltage_samples: u32,
    #[serde(default)] pub voltage_min_seen: Option<f32>,
    #[serde(default)] pub voltage_aborted: bool,
    pub recovery_prompt: Option<RecoveryPrompt>, pub logs: Vec<String>, pub error: Option<String>,
}
pub const DEFAULT_MIN_VOLTAGE_V: f32 = 12.5;
#[derive(Debug, Clone)]
pub struct AdaptiveTiming { pub base_ms: u64, pub max_ms: u64, pub consecutive_empty: u32 }
impl Default for AdaptiveTiming { fn default() -> Self { Self { base_ms: 5, max_ms: 80, consecutive_empty: 0 } } }
impl AdaptiveTiming {
    pub fn for_vpw() -> Self { Self { base_ms: 8, max_ms: 120, consecutive_empty: 0 } }
    pub fn for_can() -> Self { Self { base_ms: 3, max_ms: 60, consecutive_empty: 0 } }
    pub fn on_success(&mut self) { self.consecutive_empty = 0; }
    pub fn on_empty(&mut self) { self.consecutive_empty = self.consecutive_empty.saturating_add(1); }
    pub fn delay(&self) -> Duration { Duration::from_millis((self.base_ms * (1u64 << self.consecutive_empty.min(4))).min(self.max_ms)) }
    pub fn sleep(&self) { std::thread::sleep(self.delay()); }
}

/// Battery / supply voltage. Prefers OBDLink STVR (adapter pin) when the transport is ELM/STN,
/// falls back to Mode 01 PID 0x42 (control module voltage) on raw VPW.
pub fn read_battery_voltage(port: &mut Box<dyn SerialPort + Send>) -> Option<f32> {
    // Adapter voltage is more reliable for the flash gate on OBDLink MX+.
    if let Some(v) = crate::transport::read_adapter_voltage(port) {
        return Some(v);
    }
    // Raw VPW fallback: Mode 01 PID 0x42.
    let req = build_obd_request(0x42);
    let mut timing = AdaptiveTiming::for_vpw();
    for _ in 0..3 {
        match request_response(port, &req) {
            Ok(resp) => {
                timing.on_success();
                if let Some(data) = parse_mode01_response(&resp, 0x42) {
                    if data.len() >= 2 {
                        return Some((((data[0] as u16) << 8) | data[1] as u16) as f32 / 1000.0);
                    }
                }
            }
            Err(_) => {
                timing.on_empty();
                timing.sleep();
            }
        }
    }
    None
}

pub fn enforce_voltage_gate(port: &mut Box<dyn SerialPort + Send>, min_v: f32, logs: &mut Vec<String>) -> Result<f32, String> {
    match read_battery_voltage(port) {
        Some(v) => {
            logs.push(format!("{:.2} V (min {:.2})", v, min_v));
            if v < min_v {
                Err(format!("Voltage gate FAILED: {:.2} V", v))
            } else {
                Ok(v)
            }
        }
        None => {
            if min_v <= 0.0 {
                logs.push("Voltage gate skipped".into());
                Ok(0.0)
            } else {
                Err("Voltage gate FAILED: no STVR/ATRV or PID 0x42".into())
            }
        }
    }
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

pub fn verify_after_write(port: &mut Box<dyn SerialPort + Send>, ecu_family: &str, written: &[u8], logs: &mut Vec<String>) -> Result<(u32, bool), String> {
    logs.push(format!("verify family={} len={}", ecu_family, written.len()));
    let windows = crate::live_verify::probe_live_windows(port, ecu_family, written.len(), logs)?;
    crate::live_verify::compare_windows(written, &windows, logs)
}

/// Check a VPW response frame `[prio, tool, pcm, SID+0x40, ...]` for a positive answer to `sid`.
pub fn check_vpw_positive(resp: &[u8], sid: u8) -> Result<(), String> {
    match resp.get(3) {
        None => Err(format!("Mode 0x{:02X}: short/empty response {:02X?}", sid, resp)),
        Some(&b) if b == sid | 0x40 => Ok(()),
        Some(0x7F) => Err(format!("Mode 0x{:02X} rejected (7F) {:02X?}", sid, resp)),
        Some(&b) => Err(format!("Mode 0x{:02X}: unexpected response SID 0x{:02X}", sid, b)),
    }
}

pub const GM_VPW_CHUNK: usize = 128;

/// GM VPW Mode 34/36/37 segment write over a transport closure.
/// Unlock must succeed before anything is sent; every step needs a positive reply.
pub fn gm_vpw_write_segment<X, U, P>(xfer: &mut X, unlock: U, addr: u32, data: &[u8], mut on_chunk: P) -> Result<FlashWriteResult, String>
where
    X: FnMut(&[u8]) -> Result<Vec<u8>, String>,
    U: FnOnce() -> Result<(), String>,
    P: FnMut(usize) -> Result<(), String>,
{
    if data.is_empty() { return Err("Nothing to write".into()); }
    unlock().map_err(|e| format!("Level-2 security unlock failed: {}. Nothing erased or written.", e))?;
    let r = xfer(&build_mode34_request(addr, data.len() as u32)).map_err(|e| format!("Mode 34 request failed: {}. Nothing written.", e))?;
    check_vpw_positive(&r, 0x34).map_err(|e| format!("{}. Nothing written.", e))?;
    let mut done = 0usize;
    for chunk in data.chunks(GM_VPW_CHUNK) {
        let r = xfer(&build_mode36_chunk(chunk)).map_err(|e| format!("Mode 36 transfer failed at {} bytes: {}", done, e))?;
        check_vpw_positive(&r, 0x36).map_err(|e| format!("{} at {} bytes", e, done))?;
        done += chunk.len();
        on_chunk(done)?;
    }
    let r = xfer(&build_mode37_request()).map_err(|e| format!("Mode 37 exit failed: {}", e))?;
    check_vpw_positive(&r, 0x37)?;
    Ok(FlashWriteResult {
        bytes_written: data.len() as u32,
        blocks_written: ((data.len() + GM_VPW_CHUNK - 1) / GM_VPW_CHUNK) as u32,
        crc32_written: crc32_ieee(data),
    })
}

/// Only a full flash-image backup may proceed to write. Partial window probes and failures stay blocked.
/// Why P01 write stops after all gates pass.
/// Typed confirmation required in the UI before experimental P01 kernel write.
pub const EXPERIMENTAL_CONFIRM_PHRASE: &str = "WRITE CALIBRATION TO PCM";
pub const P01_WRITE_NEEDS_FLAG: &str = "P01 kernel write is experimental and off by default. Set experimental_p01_write and type the confirmation phrase in the UI. Frames come from public PcmHammer host source (GPL-3.0 used as protocol spec only; see COMPLETION.md). Not bench-tested.";

pub fn backup_quality_allows_write(quality: &BackupQuality) -> bool {
    matches!(quality, BackupQuality::FullImage)
}

/// Every guided flash attempt is written to the audit log, whatever the outcome.
pub fn orchestrate_guided_flash<F>(port: &mut Box<dyn SerialPort + Send>, request: GuidedFlashRequest, on_progress: F) -> Result<GuidedFlashResult, String>
where F: FnMut(FlashProgress),
{
    let image = request.tuned_bin.clone();
    let family = crate::v29_tools::resolved_family(&image).unwrap_or_else(|_| "unknown".into());
    let backup_sha = request.backup_path.as_ref().and_then(|p| std::fs::read(p).ok()).map(|b| crate::p01_backup::sha256_hex(&b));
    let r = orchestrate_guided_flash_inner(port, request, on_progress);
    let (res, err, gates, detail) = match &r {
        Ok(g) => (crate::audit_log::result_word(g.success, &g.error), g.error.clone(), g.steps_completed.clone(), g.error.clone().unwrap_or_else(|| "write completed".into())),
        Err(e) => ("failed", Some(e.clone()), vec![], e.clone()),
    };
    let _ = crate::audit_log::append(&crate::audit_log::AuditEntry {
        ts_unix_ms: crate::audit_log::now_ms(), kind: "flash".into(), family,
        os_id: crate::checksum::p01_binary_os_id(&image),
        image_sha256: if image.is_empty() { None } else { Some(crate::p01_backup::sha256_hex(&image)) },
        backup_sha256: backup_sha, gates_passed: gates, gate_failed: err, result: res.into(), detail,
    });
    r
}
