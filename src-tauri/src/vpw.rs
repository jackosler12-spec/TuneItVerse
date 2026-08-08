#![allow(unused, dead_code)]
//! vpw.rs — J1850 VPW protocol layer for LS1 PCM (P01/P59)
//! Device IDs and frame format sourced from reference/VPW.cs (UniversalPatcher)
//! PCM  = 0x10, Tool = 0xF0, Broadcast = 0xFE
//!
//! Kernel modes (post Mode 34/36/37 upload):
//!   0x3C ReadBlock — bulk memory read while kernel is resident
//!   0x20 ExitKernel
//!   0x3F TestDevicePresent
//!   0xA0 HighSpeedPrepare / 0xA1 HighSpeed
//!
//! High-speed VPW (~4× normal bit rate) requires BOTH:
//!   1. Kernel accepting 0xA0 then 0xA1
//!   2. The tool/adapter switching its physical VPW timing
//! ELM327 clones often cannot do (2); J2534 / STN / AVT usually can.

use serialport::SerialPort;
use std::io::{Read, Write};

// ── Device addresses (from VPW.cs DeviceId) ───────────────────────────────
pub const PCM_ADDR: u8 = 0x10;
pub const TOOL_ADDR: u8 = 0xF0;
pub const BROADCAST_ADDR: u8 = 0xFE;

// ── Priority bytes (from VPW.cs Priority) ────────────────────────────────
pub const PRIO_HIGH_PHYS: u8 = 0x8C;
pub const PRIO_PHYS: u8 = 0x6C;
pub const PRIO_FUNC_OBD: u8 = 0x68;
pub const FUNC_TARGET: u8 = 0x6A;

// ── Mode constants (from VPW.cs Mode) ───────────────────────────────────
pub const MODE_EXIT_KERNEL: u8 = 0x20;
pub const MODE_READ_BLOCK: u8 = 0x3C;
pub const MODE_TEST_DEVICE: u8 = 0x3F;
pub const MODE_HS_PREPARE: u8 = 0xA0;
pub const MODE_HS_ENTER: u8 = 0xA1;

/// Positive response = mode | 0x40
pub const RESP_HS_PREPARE: u8 = 0xE0; // A0 + 0x40
pub const RESP_HS_ENTER: u8 = 0xE1;   // A1 + 0x40

// ── Frame builders ──────────────────────────────────────────────────────

pub fn build_obd_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x01, pid];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_physical_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x01, pid];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_mode22_request(did_high: u8, did_low: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x22, did_high, did_low];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_dtc_request() -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x03];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_clear_dtc_request() -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x04];
    frame.push(vpw_checksum(&frame));
    frame
}

/// Mode 0x3C ReadBlock — kernel-assisted bulk memory read.
pub fn build_mode3c_read_block(address: u32, size: u16) -> Vec<u8> {
    let mut frame = vec![
        PRIO_HIGH_PHYS,
        PCM_ADDR,
        TOOL_ADDR,
        MODE_READ_BLOCK,
        ((address >> 16) & 0xFF) as u8,
        ((address >> 8) & 0xFF) as u8,
        (address & 0xFF) as u8,
        ((size >> 8) & 0xFF) as u8,
        (size & 0xFF) as u8,
    ];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_mode3f_test_device() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_TEST_DEVICE];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_mode20_exit_kernel() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_EXIT_KERNEL];
    frame.push(vpw_checksum(&frame));
    frame
}

/// Mode 0xA0 HighSpeedPrepare — tell kernel to arm high-speed VPW.
pub fn build_mode_a0_hs_prepare() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_HS_PREPARE];
    frame.push(vpw_checksum(&frame));
    frame
}

/// Mode 0xA1 HighSpeed — enter high-speed VPW (~4× bit rate).
/// Caller must also switch the tool/adapter physical layer after a positive E1.
pub fn build_mode_a1_hs_enter() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_HS_ENTER];
    frame.push(vpw_checksum(&frame));
    frame
}

// ── Checksum ────────────────────────────────────────────────────────────

pub fn vpw_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

pub fn validate_frame(frame: &[u8]) -> bool {
    if frame.len() < 2 {
        return false;
    }
    let payload = &frame[..frame.len() - 1];
    vpw_checksum(payload) == frame[frame.len() - 1]
}

// ── Response parsers ────────────────────────────────────────────────────

pub fn parse_mode01_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 {
        return None;
    }
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_frame(frame) {
        return Some(frame[5..frame.len() - 1].to_vec());
    }
    None
}

pub fn parse_mode22_response(frame: &[u8], did_high: u8, did_low: u8) -> Option<Vec<u8>> {
    if frame.len() < 7 {
        return None;
    }
    if frame[3] == 0x62 && frame[4] == did_high && frame[5] == did_low && validate_frame(frame) {
        return Some(frame[6..frame.len() - 1].to_vec());
    }
    None
}

pub fn parse_dtc_response(frame: &[u8]) -> Option<Vec<[u8; 2]>> {
    if frame.len() < 5 || frame[3] != 0x43 || !validate_frame(frame) {
        return None;
    }
    let data = &frame[5..frame.len() - 1];
    Some(data.chunks_exact(2).map(|c| [c[0], c[1]]).collect())
}

pub fn parse_mode3c_response(frame: &[u8]) -> Result<Vec<u8>, String> {
    if frame.len() < 5 {
        return Err(format!("Mode 3C response too short: {} bytes", frame.len()));
    }
    let sid_idx = if frame.len() > 3 && (frame[3] == 0x7C || frame[3] == 0x7F) {
        3
    } else if frame.first() == Some(&0x7C) || frame.first() == Some(&0x7F) {
        0
    } else {
        return Err(format!(
            "Not a Mode 3C response (SID=0x{:02X})",
            frame.get(3).copied().unwrap_or(0)
        ));
    };

    if frame[sid_idx] == 0x7F {
        let nrc = frame.get(sid_idx + 2).copied().unwrap_or(0);
        return Err(format!("Mode 3C negative response NRC 0x{:02X}", nrc));
    }

    let mut data = frame[sid_idx + 1..].to_vec();
    if data.len() > 1 && validate_frame(frame) {
        data.pop();
    }
    Ok(data)
}

/// True if `frame` looks like a positive response to 0xA0 or 0xA1.
/// Accepts SID at index 0 or 3 (with/without 3-byte VPW header).
pub fn is_positive_hs_response(frame: &[u8], expected_resp: u8) -> bool {
    if frame.is_empty() {
        return false;
    }
    frame.iter().any(|&b| b == expected_resp)
        || frame.get(3).copied() == Some(expected_resp)
        || frame.first().copied() == Some(expected_resp)
}

/// Classify a high-speed control response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HsResponse {
    PrepareOk,
    EnterOk,
    Negative,
    Unknown,
}

pub fn parse_hs_response(frame: &[u8]) -> HsResponse {
    if frame.is_empty() {
        return HsResponse::Unknown;
    }
    if is_positive_hs_response(frame, RESP_HS_PREPARE) {
        return HsResponse::PrepareOk;
    }
    if is_positive_hs_response(frame, RESP_HS_ENTER) {
        return HsResponse::EnterOk;
    }
    // Generic negative: 0x7F present
    if frame.iter().any(|&b| b == 0x7F) {
        return HsResponse::Negative;
    }
    HsResponse::Unknown
}

// ── Serial I/O ──────────────────────────────────────────────────────────

pub fn send_frame(port: &mut Box<dyn SerialPort + Send>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame).map_err(|e| format!("VPW send error: {}", e))
}

pub fn recv_frame(port: &mut Box<dyn SerialPort + Send>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 1100];
    let n = port.read(&mut buf).map_err(|e| format!("VPW recv error: {}", e))?;
    if n == 0 {
        return Err("VPW recv: no data".to_string());
    }
    Ok(buf[..n].to_vec())
}

pub fn request_response(
    port: &mut Box<dyn SerialPort + Send>,
    frame: &[u8],
) -> Result<Vec<u8>, String> {
    send_frame(port, frame)?;
    for _ in 0..4 {
        match recv_frame(port) {
            Ok(resp) if !resp.is_empty() => return Ok(resp),
            Ok(_) => continue,
            Err(e) if e.contains("no data") => continue,
            Err(e) => return Err(e),
        }
    }
    Err("VPW: no response after 4 attempts".to_string())
}

// ── Flash / Kernel protocol builders ────────────────────────────────────

pub fn build_mode34_request(address: u32, size: u32) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x34, 0x00];
    frame.extend_from_slice(&address.to_be_bytes());
    frame.extend_from_slice(&size.to_be_bytes());
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_mode36_chunk(data: &[u8]) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x36];
    frame.extend_from_slice(data);
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_mode37_request() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x37];
    frame.push(vpw_checksum(&frame));
    frame
}

pub fn build_physical_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR];
    frame.extend_from_slice(payload);
    frame.push(vpw_checksum(&frame));
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode3c_frame_structure() {
        let f = build_mode3c_read_block(0x00020000, 0x0100);
        assert_eq!(f[3], MODE_READ_BLOCK);
        assert_eq!(f[4], 0x02);
        assert_eq!(f[5], 0x00);
        assert_eq!(f[6], 0x00);
        assert_eq!(f[7], 0x01);
        assert_eq!(f[8], 0x00);
        assert!(validate_frame(&f));
    }

    #[test]
    fn mode3c_response_parse() {
        let mut frame = vec![0x8C, 0xF0, 0x10, 0x7C, 0xAA, 0xBB, 0xCC, 0xDD];
        let cs = vpw_checksum(&frame);
        frame.push(cs);
        let data = parse_mode3c_response(&frame).unwrap();
        assert_eq!(data, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn exit_kernel_frame() {
        let f = build_mode20_exit_kernel();
        assert_eq!(f[3], MODE_EXIT_KERNEL);
        assert!(validate_frame(&f));
    }

    #[test]
    fn hs_prepare_enter_frames() {
        let a0 = build_mode_a0_hs_prepare();
        assert_eq!(a0[3], MODE_HS_PREPARE);
        assert!(validate_frame(&a0));
        let a1 = build_mode_a1_hs_enter();
        assert_eq!(a1[3], MODE_HS_ENTER);
        assert!(validate_frame(&a1));
    }

    #[test]
    fn hs_response_parse() {
        let prep = vec![0x8C, 0xF0, 0x10, RESP_HS_PREPARE, 0x00];
        assert_eq!(parse_hs_response(&prep), HsResponse::PrepareOk);
        let enter = vec![0x8C, 0xF0, 0x10, RESP_HS_ENTER];
        assert_eq!(parse_hs_response(&enter), HsResponse::EnterOk);
        let neg = vec![0x8C, 0xF0, 0x10, 0x7F, 0xA0, 0x11];
        assert_eq!(parse_hs_response(&neg), HsResponse::Negative);
    }
}
