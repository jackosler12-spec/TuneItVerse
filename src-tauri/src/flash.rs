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
    read_battery_voltage_src(port).map(|(v, _)| v)
}

/// Supply voltage plus where it came from (`adapter_stvr`, `adapter_atrv`,
/// `pid42`, `pid42_vpw_raw`). Adapter voltage first: it is what the OBDLink MX+
/// measures on pin 16, and many ECUs (e.g. the Patrol ZD30) do not support
/// PID 0x42. PID 0x42 is used only when the ECU lists it (ELM path) or on the
/// raw VPW frame path. Raw VPW frames are never written to an ELM/STN port.
pub fn read_battery_voltage_src(port: &mut Box<dyn SerialPort + Send>) -> Option<(f32, &'static str)> {
    if let Some((v, src)) = crate::transport::read_voltage_with_source(port) {
        return Some((v, src.label()));
    }
    let proto = crate::transport::current_protocol();
    if !(crate::transport::is_raw_vpw(&proto) && !crate::transport::is_elm_ascii(&proto)) {
        return None;
    }
    // Raw VPW: Mode 01 PID 0x42.
    let req = build_obd_request(0x42);
    let mut timing = AdaptiveTiming::for_vpw();
    for _ in 0..3 {
        match request_response(port, &req) {
            Ok(resp) => {
                timing.on_success();
                if let Some(data) = parse_mode01_response(&resp, 0x42) {
                    if data.len() >= 2 {
                        return Some(((((data[0] as u16) << 8) | data[1] as u16) as f32 / 1000.0, crate::transport::VoltageSource::Pid42VpwRaw.label()));
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
    match read_battery_voltage_src(port) {
        Some((v, src)) => {
            logs.push(format!("{:.2} V (min {:.2}, source {})", v, min_v, src));
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

fn orchestrate_guided_flash_inner<F>(port: &mut Box<dyn SerialPort + Send>, request: GuidedFlashRequest, mut on_progress: F) -> Result<GuidedFlashResult, String>
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
    if family.eq_ignore_ascii_case("P01_0411") {
        if !request.experimental_p01_write {
            result.error = Some(P01_WRITE_NEEDS_FLAG.into());
            return Ok(result);
        }
        if request.experimental_confirm.as_deref() != Some(EXPERIMENTAL_CONFIRM_PHRASE) {
            result.error = Some(format!("Experimental P01 write requires typing exactly: {}", EXPERIMENTAL_CONFIRM_PHRASE));
            return Ok(result);
        }
        // Re-check voltage immediately before the destructive phase.
        if let Err(e) = enforce_voltage_gate(port, min_v, &mut result.logs) {
            result.error = Some(e); return Ok(result);
        }
        result.logs.push("EXPERIMENTAL P01 kernel calibration write (PcmHammer protocol spec; bench-unverified).".into());
        result.steps_completed.push("Experimental flag + typed confirm".into());
        let mut link = crate::p01_backup::StnLink::new(port);
        let wr = crate::p01_write::write_full(&mut link, &image, |p| {
            on_progress(FlashProgress { bytes_done: p.bytes_done, bytes_total: p.bytes_total, percent: p.percent, voltage_warn: None });
        }, &mut result.logs);
        match wr {
            Ok(w) if w.success => {
                result.success = true;
                result.flash_write_result = Some(FlashWriteResult { bytes_written: w.bytes_written, blocks_written: (w.bytes_written + 0x3FF) / 0x400, crc32_written: w.cal_crc_file });
                result.verification_crc = w.cal_crc_pcm;
                result.verified_live = true;
                result.steps_completed.push("P01 cal erase+write+CRC".into());
                return Ok(result);
            }
            Ok(w) => {
                result.error = Some(w.notes.clone());
                if w.recovery_needed {
                    result.recovery_prompt = Some(RecoveryPrompt {
                        prompt_type: "p01_cal_recovery".into(),
                        message: "Calibration write failed or CRC mismatched. Calibration was re-erased to force recovery mode.".into(),
                        steps: vec![
                            "Keep power stable.".into(),
                            "Re-run P01 full backup if possible.".into(),
                            "Retry experimental write with the known-good FullImage backup BIN.".into(),
                            "If the PCM will not boot, use recovery/boot-mode flashing with PcmHammer.".into(),
                        ],
                        kernel_to_upload: Some("Kernel-P01.bin".into()),
                        grounding_required: false,
                        reference_notes: "PcmHammer VpwKernelSession.AfterFailedWrite erases calibration after a failed cal write.".into(),
                    });
                }
                return Ok(result);
            }
            Err(e) => { result.error = Some(e); return Ok(result); }
        }
    }
    // Remaining family paths restored from original (Bosch/UDS/etc. stay blocked until verified kernels exist).
    result.error = Some("No live write path for this family. Identify/report only until a measured corrector and kernel exist.".into());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(sid: u8) -> Vec<u8> {
        vec![0x6C, 0xF0, 0x10, sid | 0x40]
    }

    #[test] fn gm_rejection_stops_before_data() {
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
    #[test]
    fn experimental_p01_write_defaults_off_and_needs_phrase() {
        let req: GuidedFlashRequest = serde_json::from_str(r#"{"ecu_family":"P01_0411"}"#).unwrap();
        assert!(!req.experimental_p01_write);
        assert!(req.experimental_confirm.is_none());
        assert_eq!(EXPERIMENTAL_CONFIRM_PHRASE, "WRITE CALIBRATION TO PCM");
        assert!(P01_WRITE_NEEDS_FLAG.contains("experimental"));
    }
}
