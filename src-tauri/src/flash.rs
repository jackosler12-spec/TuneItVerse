// flash.rs — Guided flash pipeline v3.9.1
use serde::{Serialize, Deserialize};
use crate::checksum::ChecksumReport;
use serialport::SerialPort;
use crate::vpw::{request_response, build_mode34_request, build_mode36_chunk, build_mode37_request, send_frame, build_obd_request, parse_mode01_response};
use crate::security::unlock_level2;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashWriteResult { pub bytes_written: u32, pub blocks_written: u32, pub crc32_written: u32 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress { pub bytes_done: u32, pub bytes_total: u32, pub percent: u8 }
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
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPrompt { pub prompt_type: String, pub message: String, pub steps: Vec<String>, pub kernel_to_upload: Option<String>, pub grounding_required: bool, pub reference_notes: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedFlashResult {
    pub success: bool, pub steps_completed: Vec<String>, pub backup: Option<BackupResult>,
    pub checksum_report: Option<ChecksumReport>, pub flash_write_result: Option<FlashWriteResult>,
    pub verification_crc: Option<u32>, pub verified_live: bool, pub voltage_at_start: Option<f32>,
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
    let windows = crate::live_verify::probe_live_windows(port, ecu_family, written.len(), logs);
    crate::live_verify::compare_windows(written, &windows, logs)
}
pub fn orchestrate_guided_flash<F>(port: &mut Box<dyn SerialPort + Send>, request: GuidedFlashRequest, mut on_progress: F) -> Result<GuidedFlashResult, String>
where F: FnMut(FlashProgress),
{
    let min_v = request.min_voltage_v.unwrap_or(DEFAULT_MIN_VOLTAGE_V);
    let mut result = GuidedFlashResult { success: false, steps_completed: vec![], backup: None, checksum_report: None, flash_write_result: None, verification_crc: None, verified_live: false, voltage_at_start: None, recovery_prompt: None, logs: vec!["Guided flash: identifying image…".into()], error: None };
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
        result.logs.push(format!(
            "UI family '{}' overridden by identify '{}'",
            request.ecu_family, family
        ));
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
    if request.auto_correct_checksum {
        match crate::checksum::correct_checksums(&image) {
            Ok(corrected) => { result.checksum_report = Some(corrected.report.clone()); image = corrected.data; }
            Err(e) => { result.logs.push(format!("Checksum correct skipped: {}", e)); }
        }
    }
    if request.perform_backup {
        let w = crate::live_verify::attempt_live_backup(port, &family, image.len(), &mut result.logs);
        let quality = if w.failed { BackupQuality::Failed } else { BackupQuality::PartialDidOnly };
        let backup = BackupResult { path: w.path, quality, bytes: w.bytes, crc32: w.crc32, notes: w.notes };
        if w.failed { result.error = Some("Backup failed. Refusing write.".into()); result.backup = Some(backup); return Ok(result); }
        result.backup = Some(backup);
        result.steps_completed.push("Backup captured".into());
    }
    let fam = family.to_ascii_uppercase();
    if fam.contains("HONDA") || fam.contains("KEIHIN") {
        result.error = Some("Honda write path is not enabled. No verified kernel or corrector.".into());
        return Ok(result);
    }
    let bosch_uds = fam.contains("EDC") || fam.contains("MED") || fam.contains("BOSCH")
        || fam.contains("DELPHI") || fam.contains("DCM") || fam.contains("SID");
    let gm_vpw = fam.contains("P01") || fam.contains("GM") || fam.contains("P59");
    if !bosch_uds && !gm_vpw {
        result.error = Some(format!("No verified write path for family {}", family));
        return Ok(result);
    }
    if bosch_uds {
        let lvl = crate::security::BoschSecurityLevel::from_str("programming");
        match crate::security::bosch_uds_unlock_full(port, &family, lvl) {
            Ok(msg) => result.logs.push(format!("Bosch UDS unlock: {}", msg)),
            Err(e) => { result.error = Some(format!("Bosch UDS unlock failed: {}", e)); return Ok(result); }
        }
        let cal_addr: u32 = if fam.contains("ME7") { 0x0001_8000 } else { 0x0008_0000 };
        result.logs.push(format!("UDS 0x34/36/37 download at 0x{:06X} ({} bytes)", cal_addr, image.len()));
        let write = crate::uds::download_image(
            port,
            crate::uds::Alfi::ADDR4_SIZE4,
            cal_addr,
            &image,
            true,
            |done, total| {
                on_progress(FlashProgress {
                    bytes_done: done,
                    bytes_total: total,
                    percent: ((done as u64 * 100) / total.max(1) as u64) as u8,
                });
            },
        );
        if let Err(e) = write {
            result.error = Some(e);
            return Ok(result);
        }
        result.flash_write_result = Some(FlashWriteResult {
            bytes_written: image.len() as u32,
            blocks_written: ((image.len() + 0x401) / 0x400) as u32,
            crc32_written: crc32_ieee(&image),
        });
    } else {
        let _ = unlock_level2(port);
        let cal_addr: u32 = 0x0002_0000;
        if let Err(e) = send_frame(port, &build_mode34_request(cal_addr, image.len() as u32)) { result.error = Some(e); return Ok(result); }
        let timing = AdaptiveTiming::for_vpw(); timing.sleep();
        let chunk_size = 128; let total = image.len();
        for (i, chunk) in image.chunks(chunk_size).enumerate() {
            if i > 0 && i % 10 == 0 { if let Err(e) = enforce_voltage_gate(port, min_v, &mut result.logs) { result.error = Some(e); return Ok(result); } }
            if let Err(e) = send_frame(port, &build_mode36_chunk(chunk)) { result.error = Some(e); return Ok(result); }
            let done = ((i + 1) * chunk_size).min(total);
            on_progress(FlashProgress { bytes_done: done as u32, bytes_total: total as u32, percent: ((done * 100) / total.max(1)) as u8 });
            timing.sleep();
        }
        let _ = send_frame(port, &build_mode37_request());
        result.flash_write_result = Some(FlashWriteResult { bytes_written: image.len() as u32, blocks_written: ((image.len() + chunk_size - 1) / chunk_size) as u32, crc32_written: crc32_ieee(&image) });
    }
    match verify_after_write(port, &family, &image, &mut result.logs) {
        Ok((crc, matched)) => { result.verification_crc = Some(crc); result.verified_live = matched; result.success = matched; }
        Err(e) => {
            result.error = Some(e);
            result.success = request.accept_unverified_write;
            result.verified_live = false;
        }
    }
    Ok(result)
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
}
