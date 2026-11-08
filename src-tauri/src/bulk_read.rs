//! bulk_read.rs — Multi-window UDS Mode 23 probe for larger partial dumps.
//!
//! Collects sequential windows to approach better coverage for Bosch families.
//! Still PartialDidOnly until a full kernel or verified session exists.
//! Not hardware-tested. Offline tools remain fully operational.

use serialport::SerialPort;
use crate::uds::{Alfi, read_memory_by_address};
use crate::live_verify::{LiveWindow, WindowBackup};

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

pub fn collect_uds_windows(
    port: &mut Box<dyn SerialPort + Send>,
    start_addr: u32,
    total_bytes: usize,
    window_size: u32,
    use_elm: bool,
) -> Result<Vec<LiveWindow>, String> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < total_bytes {
        let addr = start_addr.saturating_add(offset as u32);
        let size = window_size.min((total_bytes - offset) as u32);
        match read_memory_by_address(port, Alfi::ADDR4_SIZE2, addr, size, use_elm) {
            Ok(data) if !data.is_empty() => {
                out.push(LiveWindow {
                    label: format!("win_{:06X}", addr),
                    offset,
                    data,
                    method: "UDS 0x23 multi-window".into(),
                });
                offset += size as usize;
            }
            Ok(_) => return Err(format!("Empty response at 0x{:06X}", addr)),
            Err(e) => return Err(format!("Mode 23 failed at 0x{:06X}: {}", addr, e)),
        }
    }
    Ok(out)
}

/// Attempt a larger multi-window probe. Limited to 32 KiB to stay practical.
/// Returns PartialDidOnly quality. Not a full dump.
pub fn attempt_bulk_probe(
    port: &mut Box<dyn SerialPort + Send>,
    ecu_family: &str,
    expected_len: usize,
    logs: &mut Vec<String>,
) -> WindowBackup {
    let (cal_addr, _) = match crate::live_verify::live_address_plan(ecu_family) {
        Ok(p) => p,
        Err(e) => return WindowBackup { failed: true, path: String::new(), bytes: 0, crc32: None, notes: e },
    };
    // Expand probe size: up to 32KB or expected, whichever smaller.
    let probe_bytes = expected_len.min(0x8000);
    match collect_uds_windows(port, cal_addr, probe_bytes, 64, true) {
        Ok(windows) => {
            let mut crc_src = Vec::new();
            for w in &windows { crc_src.extend_from_slice(&w.data); }
            logs.push(format!("Bulk probe collected {} bytes in {} windows (PartialDidOnly)", crc_src.len(), windows.len()));
            WindowBackup {
                failed: false,
                path: format!("bulk-probe:{}", windows.len()),
                bytes: crc_src.len() as u32,
                crc32: Some(crc32_ieee(&crc_src)),
                notes: format!("Multi-window probe: {} bytes across {} windows. Still PartialDidOnly. Not a full dump. Not hardware-tested.", crc_src.len(), windows.len()),
            }
        }
        Err(e) => WindowBackup { failed: true, path: String::new(), bytes: 0, crc32: None, notes: e },
    }
}
