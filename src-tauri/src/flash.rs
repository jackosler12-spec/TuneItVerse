// flash.rs — Guided flash pipeline with Priority 0 safety gates
// v3.3.0: user_confirmed_risks defaults FALSE. Never flash offline.

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
use std::time::{Duration, Instant};

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

fn default_true() -> bool { true }
fn default_false() -> bool { false }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidedFlashRequest {
    pub ecu_family: String,
    #[serde(alias = "bin_bytes", alias = "tuned_bin", default)]
    pub tuned_bin: Vec<u8>,
    #[serde(alias = "do_backup", alias = "perform_backup", default = "default_true")]
    pub perform_backup: bool,
    #[serde(alias = "auto_correct", alias = "auto_correct_checksum", default = "default_true")]
    pub auto_correct_checksum: bool,
    #[serde(default = "default_true")]
    pub enable_recovery_prompts: bool,
    /// Fail-closed: omit / false refuses the write. UI must send true after checkboxes.
    #[serde(alias = "risks", alias = "user_confirmed_risks", default = "default_false")]
    pub user_confirmed_risks: bool,
    pub min_voltage_v: Option<f32>,
    pub prefer_high_speed: Option<bool>,
}
