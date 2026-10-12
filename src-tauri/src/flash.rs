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
pub fn read_battery_voltage(port: &mut Box<dyn SerialPort + Send>) -> Option<f32> {
    let req = build_obd_request(0x42);
    let mut timing = AdaptiveTiming::for_vpw();
    for _ in 0..3 {
        match request_response(port, &req) {
            Ok(resp) => {
                timing.on_success();
                if let Some(data) = parse_mode01_response(&resp, 0x42) {
                    if data.len() >= 2 { return Some((((data[0] as u16) << 8) | data[1] as u16) as f32 / 1000.0); }
                }
            }
            Err(_) => { timing.on_empty(); timing.sleep(); }
        }
    }
    None
}
pub fn enforce_voltage_gate(port: &mut Box<dyn SerialPort + Send>, min_v: f32, logs: &mut Vec<String>) -> Result<f32, String> {
    match read_battery_voltage(port) {
        Some(v) => { logs.push(format!("{:.2} V (min {:.2})", v, min_v)); if v < min_v { Err(format!("Voltage gate FAILED: {:.2} V", v)) } else { Ok(v) } }
        None => if min_v <= 0.0 { logs.push("Voltage gate skipped".into()); Ok(0.0) } else { Err("Voltage gate FAILED: no PID 0x42".into()) },
    }
}
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data { crc ^= byte as u32; for _ in 0..8 { if crc & 1 != 0 { crc = (crc >> 1) ^ 0xEDB8_8320; } else { crc >>= 1; } } }
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
pub fn backup_quality_allows_write(quality: &BackupQuality) -> bool {
    matches!(quality, BackupQuality::FullImage)
}

pub fn orchestrate_guided_flash<F>(port: &mut Box<dyn SerialPort + Send>, request: GuidedFlashRequest, mut on_progress: F) -> Result<GuidedFlashResult, String>
where F: FnMut(FlashProgress),
{
    let min_v = request.min_voltage_v.unwrap_or(DEFAULT_MIN_VOLTAGE_V);
    let mut result = GuidedFlashResult { success: false, steps_completed: vec![], backup: None, checksum_report: None, flash_write_result: None, verification_crc: None, verified_live: false, voltage_at_start: None, voltage_samples: 0, voltage_min_seen: None, voltage_aborted: false, recovery_prompt: None, logs: vec!["Guided flash: identifying image...".into()], error: None };
    let mut supply = crate::voltage_watch::VoltageWatch::default();
    if !request.user_confirmed_risks { result.error = Some("Risks not confirmed".into()); return Ok(result); }
    if request.tuned_bin.is_empty() { result.error = Some("Empty tuned_bin".into()); return Ok(result); }
    let family = match crate::v29_tools::resolved_family(&request.tuned_bin) {
        Ok(f) => f,
        Err(e) => {
            result.error = Some(format!("Identify refused write: {}", e));
            return Ok(result);
        }
    };
    if !request.ecu_family.is_empty()
        && !request.ecu_family.eq_ignore_ascii_case(&family)
        && !request.ecu_family.eq_ignore_ascii_case("auto")
    {
        result.logs.push(format!("UI family '{}' overridden by identify '{}'", request.ecu_family, family));
    }
    if !crate::ecu_database::write_path_live(&family) {
        result.error = Some(format!(
            "No live write path for {}. Identify/report only until a measured corrector and kernel exist.",
            family
        ));
        return Ok(result);
    }
    result.logs.push(format!("Guided flash starting for {}", family));
    match enforce_voltage_gate(port, min_v, &mut result.logs) {
        Ok(v) => { result.voltage_at_start = Some(v); result.steps_completed.push(format!("Voltage {:.2} V", v)); }
        Err(e) => { result.error = Some(e); return Ok(result); }
    }
    let mut image = request.tuned_bin.clone();
    if crate::cs_guard::honda_blocks_p01_corrector(&image) {
        result.error = Some("Honda OS string on this image. Refusing P01 additive and write.".into());
        return Ok(result);
    }
    if crate::cs_guard::p59_blocks_p01_corrector(&image) {
        result.error = Some("P59 OS string on this image. P01 additive and write stay blocked until measured P59 CS words and kernel exist.".into());
        return Ok(result);
    }
    if request.auto_correct_checksum {
        match crate::checksum::correct_checksums(&image) {
            Ok(corrected) => { result.checksum_report = Some(corrected.report.clone()); image = corrected.data; }
            Err(e) => {
                result.error = Some(format!("Checksum correction requested but failed: {}. Refusing write of an uncorrected image.", e));
                return Ok(result);
            }
        }
    }
    if !request.perform_backup {
        result.error = Some("Backup required before write. Refusing flash without a full-image backup.".into());
        return Ok(result);
    }
    if family.eq_ignore_ascii_case("P01_0411") {
        let gate = request.backup_path.as_deref()
            .ok_or_else(|| "No P01 full backup. Run the P01 full read-back backup first.".to_string())
            .and_then(crate::p01_backup::load_verified_backup)
            .and_then(|b| crate::p01_backup::check_write_os(&image, &b).map(|_| b));
        match gate {
            Ok(b) => {
                result.logs.push(format!("Backup {} SHA-256 {} FullImage", b.path, b.sha256));
                result.backup = Some(BackupResult { path: b.path, quality: BackupQuality::FullImage, bytes: b.bytes, crc32: None, notes: format!("sha256 {}", b.sha256) });
                result.steps_completed.push("Full-image backup verified".into());
            }
            Err(e) => {
                result.backup = Some(BackupResult { path: request.backup_path.clone().unwrap_or_default(), quality: BackupQuality::Failed, bytes: 0, crc32: None, notes: e.clone() });
                result.error = Some(format!("Backup gate refused write: {}", e));
                return Ok(result);
            }
        }
    } else {
        let w = crate::live_verify::attempt_live_backup(port, &family, image.len(), &mut result.logs);
        // attempt_live_backup is windowed only today, so non-failed captures stay PartialDidOnly.
        // FullImage is reserved for a future kernel Mode 3C / full dump path.
        let quality = if w.failed { BackupQuality::Failed } else { BackupQuality::PartialDidOnly };
        let backup = BackupResult { path: w.path, quality: quality.clone(), bytes: w.bytes, crc32: w.crc32, notes: w.notes };
        result.backup = Some(backup);
        if !backup_quality_allows_write(&quality) {
            let msg = match quality {
                BackupQuality::Failed => "Backup failed. Refusing write.".to_string(),
                BackupQuality::PartialDidOnly => {
                    "Backup is PartialDidOnly (windowed probe, not a full flash dump). Refusing write until a FullImage backup exists.".to_string()
                }
                BackupQuality::FullImage => unreachable!("FullImage always allows write"),
            };
            result.error = Some(msg);
            return Ok(result);
        }
        result.steps_completed.push("Full-image backup captured".into());
    }
    let fam = family.to_ascii_uppercase();
    if fam.contains("HONDA") || fam.contains("KEIHIN") {
        result.error = Some("Honda write path is not enabled. No verified kernel or corrector.".into());
        return Ok(result);
    }
    let bosch_uds = fam.contains("EDC") || fam.contains("MED") || fam.contains("BOSCH")
        || fam.contains("DELPHI") || fam.contains("DCM") || fam.contains("SID");
    if fam.contains("P59") {
        result.error = Some("GM P59 write path is not enabled. No measured checksum words or kernel on this tree.".into());
        return Ok(result);
    }
    let gm_vpw = fam.contains("P01") || (fam.contains("GM") && !fam.contains("P59"));
    if !bosch_uds && !gm_vpw {
        result.error = Some(format!("No verified write path for family {}", family));
        return Ok(result);
    }
    // Segment comes from the ECU database before any unlock or erase; the BIN
    // size must match the family and only the segment bytes are written.
    let seg = match crate::ecu_database::flash_segment(&family) {
        Ok(s) => s,
        Err(e) => { result.error = Some(e); return Ok(result); }
    };
    let cal_addr = seg.start;
    let write_data: Vec<u8> = match seg.slice(&image) {
        Ok(d) => d.to_vec(),
        Err(e) => { result.error = Some(e); return Ok(result); }
    };
    result.logs.push(format!("Write segment 0x{:06X}..0x{:06X} ({} bytes of {}-byte image)", seg.start, seg.start + seg.length - 1, write_data.len(), image.len()));
    if bosch_uds {
        let lvl = crate::security::BoschSecurityLevel::from_str("programming");
        match crate::security::bosch_uds_unlock_full(port, &family, lvl) {
            Ok(msg) => result.logs.push(format!("Bosch UDS unlock: {}", msg)),
            Err(e) => { result.error = Some(format!("Bosch UDS unlock failed: {}", e)); return Ok(result); }
        }
        result.logs.push(format!("UDS 0x34/36/37 download at 0x{:06X} ({} bytes)", cal_addr, write_data.len()));
        result.logs.push("Mid-write supply monitor: J2534 Vbatt only. Serial PID 0x42 is not sent during the transfer.".into());
        let write = crate::uds::download_image(
            port,
            crate::uds::Alfi::ADDR4_SIZE4,
            cal_addr,
            &write_data,
            true,
            |done, total| {
                let voltage_warn = sample_supply(min_v, &mut supply)?;
                on_progress(FlashProgress {
                    bytes_done: done,
                    bytes_total: total,
                    percent: ((done as u64 * 100) / total.max(1) as u64) as u8,
                    voltage_warn,
                });
                Ok(())
            },
        );
        stamp_supply(&mut result, &supply);
        if let Err(e) = write {
            if e.contains("sag") { result.voltage_aborted = true; }
            result.error = Some(e);
            return Ok(result);
        }
        match enforce_voltage_gate(port, min_v, &mut result.logs) {
            Ok(v) => result.logs.push(format!("Post-write voltage {:.2} V", v)),
            Err(e) => result.logs.push(format!("WARNING post-write voltage check failed: {}", e)),
        }
        result.flash_write_result = Some(FlashWriteResult {
            bytes_written: write_data.len() as u32,
            blocks_written: ((write_data.len() + 0x3FF) / 0x400) as u32,
            crc32_written: crc32_ieee(&write_data),
        });
    } else {
        result.logs.push("Mid-write supply monitor: J2534 Vbatt only. Serial PID 0x42 is not sent during the transfer.".into());
        let timing = AdaptiveTiming::for_vpw();
        let total = write_data.len();
        let mut aborted_v = false;
        let write = {
            let port_cell = std::cell::RefCell::new(&mut *port);
            let mut xfer = |f: &[u8]| request_response(&mut **port_cell.borrow_mut(), f);
            let unlock = || unlock_level2(&mut **port_cell.borrow_mut()).map(|_| ());
            gm_vpw_write_segment(&mut xfer, unlock, cal_addr, &write_data, |done| {
                let voltage_warn = sample_supply(min_v, &mut supply).map_err(|e| { aborted_v = true; e })?;
                on_progress(FlashProgress { bytes_done: done as u32, bytes_total: total as u32, percent: ((done * 100) / total.max(1)) as u8, voltage_warn });
                timing.sleep();
                Ok(())
            })
        };
        match write {
            Ok(w) => result.flash_write_result = Some(w),
            Err(e) => { result.voltage_aborted = aborted_v; result.error = Some(e); stamp_supply(&mut result, &supply); return Ok(result); }
        }
    }
    match verify_after_write(port, &family, &image, &mut result.logs) {
        Ok((crc, matched)) => { result.verification_crc = Some(crc); result.verified_live = matched; result.success = matched; }
        Err(e) => {
            result.error = Some(e);
            result.success = request.accept_unverified_write;
            result.verified_live = false;
        }
    }
    stamp_supply(&mut result, &supply);
    Ok(result)
}

fn sample_supply(min_v: f32, watch: &mut crate::voltage_watch::VoltageWatch) -> Result<Option<f32>, String> {
    if !crate::j2534::is_device_open() {
        return Ok(None);
    }
    let v = match crate::j2534::j2534_read_vbatt() {
        Ok(v) if v > 0.0 => v,
        _ => return Ok(None),
    };
    watch.observe(v);
    match crate::voltage_watch::classify(v, min_v) {
        crate::voltage_watch::SupplyClass::Abort => Err(crate::voltage_watch::sag_message(v, min_v)),
        crate::voltage_watch::SupplyClass::Warn => { watch.warns = watch.warns.saturating_add(1); Ok(Some(v)) }
        crate::voltage_watch::SupplyClass::Ok => Ok(None),
    }
}

fn stamp_supply(result: &mut GuidedFlashResult, watch: &crate::voltage_watch::VoltageWatch) {
    result.voltage_samples = watch.samples;
    result.voltage_min_seen = watch.min_seen;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn risks_default_is_false() {
        let req: GuidedFlashRequest = serde_json::from_str(r#"{"ecu_family":"P01_0411"}"#).unwrap();
        assert!(!req.user_confirmed_risks);
        assert!(!req.accept_unverified_write);
    }
    #[test] fn unidentified_512k_is_not_p01() {
        let img = vec![0u8; 524288];
        assert!(crate::v29_tools::resolved_family(&img).is_err());
    }
    #[test] fn write_path_is_fail_closed() {
        assert!(crate::ecu_database::write_path_live("P01_0411"));
        assert!(crate::ecu_database::write_path_live("EDC16C41"));
        assert!(!crate::ecu_database::write_path_live("EDC17_COMMON"));
        assert!(!crate::ecu_database::write_path_live("MED17_COMMON"));
        assert!(!crate::ecu_database::write_path_live("TRANSTRON_4HK1"));
    }
    fn ok(sid: u8) -> Vec<u8> { vec![0x6C, 0xF0, 0x10, sid | 0x40] }

    #[test] fn gm_unlock_failure_sends_nothing() {
        let mut sent: Vec<Vec<u8>> = vec![];
        let mut x = |f: &[u8]| { sent.push(f.to_vec()); Ok(ok(f[3])) };
        let e = gm_vpw_write_segment(&mut x, || Err("NRC 0x35".into()), 0x8000, &[1; 300], |_| Ok(())).unwrap_err();
        assert!(e.contains("unlock failed") && e.contains("Nothing erased"));
        assert!(sent.is_empty());
    }
    #[test] fn gm_mode34_rejection_stops_before_data() {
        let mut sent: Vec<Vec<u8>> = vec![];
        let mut x = |f: &[u8]| { sent.push(f.to_vec()); Ok(vec![0x6C, 0xF0, 0x10, 0x7F, 0x34, 0x22]) };
        assert!(gm_vpw_write_segment(&mut x, || Ok(()), 0x8000, &[1; 300], |_| Ok(())).is_err());
        assert_eq!(sent.len(), 1);
    }
    #[test] fn gm_happy_path_writes_segment_at_address() {
        let mut sent: Vec<Vec<u8>> = vec![];
        let mut x = |f: &[u8]| { sent.push(f.to_vec()); Ok(ok(f[3])) };
        let mut progress = vec![];
        let w = gm_vpw_write_segment(&mut x, || Ok(()), 0x8000, &[7; 300], |d| { progress.push(d); Ok(()) }).unwrap();
        assert_eq!(w.bytes_written, 300);
        assert_eq!(sent[0], build_mode34_request(0x8000, 300));
        assert_eq!(sent.len(), 1 + 3 + 1);
        assert_eq!(sent[4], build_mode37_request());
        assert_eq!(progress, vec![128, 256, 300]);
    }
    #[test] fn gm_chunk_nack_or_empty_aborts() {
        let mut n = 0;
        let mut x = |f: &[u8]| { n += 1; if n == 3 { Ok(vec![]) } else { Ok(ok(f[3])) } };
        let e = gm_vpw_write_segment(&mut x, || Ok(()), 0x8000, &[1; 400], |_| Ok(())).unwrap_err();
        assert!(e.contains("Mode 0x36") && e.contains("128 bytes"));
    }
    #[test] fn gm_voltage_abort_propagates() {
        let mut x = |f: &[u8]| Ok(ok(f[3]));
        assert!(gm_vpw_write_segment(&mut x, || Ok(()), 0x8000, &[1; 400], |_| Err("sag".into())).is_err());
    }
    #[test] fn real_p01_bin_segment_slice() {
        let img = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../reference/LS1 PCM P01 12225074.bin")).unwrap();
        let seg = crate::ecu_database::flash_segment("P01_0411").unwrap();
        let w = seg.slice(&img).unwrap();
        assert_eq!(w, &img[0x8000..0x20000]);
    }
    #[test] fn partial_or_failed_backup_blocks_write() {
        assert!(!backup_quality_allows_write(&BackupQuality::PartialDidOnly));
        assert!(!backup_quality_allows_write(&BackupQuality::Failed));
        assert!(backup_quality_allows_write(&BackupQuality::FullImage));
    }
}
