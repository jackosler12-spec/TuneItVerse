//! Windowed live readback for backup + post-flash verify.
//! VPW Mode 3C, UDS 0x23, KWP 0x23. Honest: windows are not a full dump.

use serialport::SerialPort;
use crate::vpw::{request_response, build_mode3c_read_block, parse_mode3c_response};
use crate::flash::{BackupQuality, BackupResult};

#[derive(Debug, Clone)]
pub struct LiveWindow {
    pub label: String,
    pub offset: usize,
    pub data: Vec<u8>,
    pub method: String,
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 { crc = (crc >> 1) ^ 0xEDB8_8320; } else { crc >>= 1; }
        }
    }
    !crc
}

pub fn probe_live_windows(
    port: &mut Box<dyn SerialPort + Send>,
    ecu_family: &str,
    written_len: usize,
    logs: &mut Vec<String>,
) -> Vec<LiveWindow> {
    let fam = ecu_family.to_ascii_uppercase();
    let mut out = Vec::new();
    let cal_addr: u32 = if fam.contains("EDC") || fam.contains("MED") { 0x0008_0000 } else { 0x0002_0000 };
    let starts = [
        (cal_addr, "cal_start"),
        (cal_addr.saturating_add(0x1000), "cal_plus_4k"),
    ];

    if fam.contains("P01") || fam.contains("P59") || fam.contains("GM") {
        for (addr, label) in &starts {
            match request_response(port, &build_mode3c_read_block(*addr, 64)) {
                Ok(resp) => match parse_mode3c_response(&resp) {
                    Ok(data) if !data.is_empty() => {
                        let off = if (*addr as usize) < written_len { *addr as usize } else { 0 };
                        logs.push(format!("Mode 3C answered at 0x{:06X} ({} bytes)", addr, data.len()));
                        out.push(LiveWindow { label: label.to_string(), offset: off, data, method: "VPW Mode 3C".into() });
                    }
                    Ok(_) => logs.push(format!("Mode 3C empty at 0x{:06X}", addr)),
                    Err(e) => logs.push(format!("Mode 3C parse at 0x{:06X}: {}", addr, e)),
                },
                Err(e) => logs.push(format!("Mode 3C no answer at 0x{:06X}: {}", addr, e)),
            }
        }
    }

    if fam.contains("EDC") || fam.contains("MED") || fam.contains("BOSCH") || out.is_empty() {
        for (addr, label) in &starts {
            if let Ok(frame) = crate::uds::build_read_memory(crate::uds::Alfi(0x24), *addr, 64) {
                match request_response(port, &frame) {
                    Ok(resp) => match crate::uds::parse_response(0x63, &resp) {
                        Ok(data) if !data.is_empty() => {
                            let off = if (*addr as usize) < written_len { *addr as usize } else { 0 };
                            logs.push(format!("UDS 0x23 answered at 0x{:06X} ({} bytes)", addr, data.len()));
                            out.push(LiveWindow { label: label.to_string(), offset: off, data, method: "UDS 0x23".into() });
                        }
                        Ok(_) => logs.push(format!("UDS 0x23 empty at 0x{:06X}", addr)),
                        Err(e) => logs.push(format!("UDS 0x23 at 0x{:06X}: NRC 0x{:02X} {}", addr, e.nrc, e.description())),
                    },
                    Err(e) => logs.push(format!("UDS 0x23 no answer at 0x{:06X}: {}", addr, e)),
                }
            }
        }
        for (addr, label) in &starts {
            match crate::kwp::kwp_read_memory(port, *addr, 64) {
                Ok(data) if data.len() > 4 => {
                    let off = if (*addr as usize) < written_len { *addr as usize } else { 0 };
                    logs.push(format!("KWP 0x23 answered at 0x{:06X} ({} bytes)", addr, data.len()));
                    out.push(LiveWindow { label: format!("{}_kwp", label), offset: off, data, method: "KWP 0x23".into() });
                }
                Ok(_) => logs.push(format!("KWP 0x23 short at 0x{:06X}", addr)),
                Err(e) => logs.push(format!("KWP 0x23 at 0x{:06X}: {}", addr, e)),
            }
        }
    }
    out
}

pub fn compare_windows(written: &[u8], windows: &[LiveWindow], logs: &mut Vec<String>) -> Result<(u32, bool), String> {
    let expected = crc32_ieee(written);
    if windows.is_empty() {
        return Err("Live verification got no Mode 23 / Mode 3C / KWP 0x23 windows. Kernel or UDS session required.".into());
    }
    let mut matched = 0usize;
    let mut compared = 0usize;
    for w in windows {
        let end = (w.offset + w.data.len()).min(written.len());
        if w.offset >= written.len() || end <= w.offset { continue; }
        let slice = &written[w.offset..end];
        let n = slice.len().min(w.data.len());
        compared += n;
        matched += slice.iter().zip(w.data.iter()).take(n).filter(|(a, b)| a == b).count();
        logs.push(format!("Live window {} @0x{:06X} {} bytes via {} — match pending", w.label, w.offset, w.data.len(), w.method));
    }
    if compared == 0 {
        return Err("Windows returned but none overlapped the written image.".into());
    }
    let ok = matched == compared && compared >= 16;
    logs.push(format!("Live compare {} / {} bytes (verified_live={}) expected_crc=0x{:08X}", matched, compared, ok, expected));
    Ok((expected, ok))
}

pub fn attempt_live_backup(
    port: &mut Box<dyn SerialPort + Send>,
    ecu_family: &str,
    expected_len: usize,
    logs: &mut Vec<String>,
) -> BackupResult {
    let windows = probe_live_windows(port, ecu_family, expected_len.max(1), logs);
    if windows.is_empty() {
        return BackupResult {
            path: String::new(),
            quality: BackupQuality::Failed,
            bytes: 0,
            crc32: None,
            notes: "No Mode 23 / 3C windows. Need kernel (P01) or UDS session (Bosch). Not a full-image backup.".into(),
        };
    }
    let mut crc_src = Vec::new();
    for w in &windows { crc_src.extend_from_slice(&w.data); }
    BackupResult {
        path: format!("live-windows:{}", windows.len()),
        quality: BackupQuality::PartialDidOnly,
        bytes: crc_src.len() as u32,
        crc32: Some(crc32_ieee(&crc_src)),
        notes: format!("Collected {} bytes across {} live windows. PartialDidOnly is honest — not a full flash dump.", crc_src.len(), windows.len()),
    }
}
