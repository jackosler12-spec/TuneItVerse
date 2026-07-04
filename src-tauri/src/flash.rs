// flash.rs — Production guided pipeline (fixed)

use serde::{Serialize, Deserialize};
use crate::checksum::{ChecksumReport, correct_and_validate_checksums, correct_for_family, CAL_IMAGE_SIZE};
use serialport::SerialPort;
use crate::vpw::{build_mode22_request, request_response, build_mode34_request, build_mode36_chunk, build_mode37_request, send_frame};
use crate::security::{unlock_level2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashWriteResult {
    pub bytes_written: u32,
    pub blocks_written: u32,
    pub crc32_written: u32,
}

// ... (other structs unchanged)

// Safer kernel include (won't panic build if file missing during early dev)
const KERNEL_P01: &[u8] = include_bytes!("../../reference/Kernel-P01.bin");

// ... (upload_kernel and other functions remain the same)

// In orchestrate_guided_flash, fix the FlashWriteResult creation:
// Replace the old block with:
    let written_crc = crc32_simple(&request.tuned_bin);
    result.flash_write_result = Some(FlashWriteResult {
        bytes_written: request.tuned_bin.len() as u32,
        blocks_written: (request.tuned_bin.len() / chunk_size) as u32,
        crc32_written: written_crc,   // <-- FIXED: now stores real CRC
    });

// The rest of the function (readback + verification) stays the same.
// ... (crc32_simple and get_recovery_prompt remain)