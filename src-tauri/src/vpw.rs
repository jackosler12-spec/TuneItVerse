#![allow(unused, dead_code)]
//! vpw.rs — J1850 VPW protocol layer for LS1 PCM (P01/P59)
//! Device IDs and frame format sourced from reference/VPW.cs (UniversalPatcher)
//! PCM  = 0x10, Tool = 0xF0, Broadcast = 0xFE

use serialport::SerialPort;
use std::io::{Read, Write};

// ── Device addresses (from VPW.cs DeviceId) ───────────────────────────────
pub const PCM_ADDR: u8 = 0x10;
pub const TOOL_ADDR: u8 = 0xF0;
pub const BROADCAST_ADDR: u8 = 0xFE;

// ── Priority bytes (from VPW.cs Priority) ────────────────────────────────
/// High priority physical addressing (3-byte header, GM IFR)
pub const PRIO_HIGH_PHYS: u8 = 0x8C;
/// Normal functional OBD-II request (Mode 01 PIDs)
pub const PRIO_FUNC_OBD: u8 = 0x68;
/// Functional target address (all ECUs on bus)
pub const FUNC_TARGET: u8 = 0x6A;

// ── Frame builder ─────────────────────────────────────────────────────────

/// Build a standard OBD-II Mode 01 PID request frame.
/// Format: [Priority] [Target] [Source] [Mode=0x01] [PID] [Checksum]
/// Matches J1850 VPW / ISO 15765 functional addressing used by LS1 PCM.
pub fn build_obd_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x01, pid];
    let checksum = vpw_checksum(&frame);
    frame.push(checksum);
    frame
}

/// Build a physical-addressed Mode 01 request (direct to PCM at 0x10).
pub fn build_physical_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x01, pid];
    let checksum = vpw_checksum(&frame);
    frame.push(checksum);
    frame
}

/// Build a Mode 22 (Read Data by Identifier) request — used for LS1-specific
/// extended parameters not covered by Mode 01 OBD-II PIDs.
/// e.g. injector pulse width, knock retard, MAF g/s.
pub fn build_mode22_request(did_high: u8, did_low: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x22, did_high, did_low];
    let checksum = vpw_checksum(&frame);
    frame.push(checksum);
    frame
}

/// Build a Mode 3 (Request Emission-Related DTCs) frame.
pub fn build_dtc_request() -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x03];
    let checksum = vpw_checksum(&frame);
    frame.push(checksum);
    frame
}

/// Build a Mode 4 (Clear DTCs) frame.
pub fn build_clear_dtc_request() -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x04];
    let checksum = vpw_checksum(&frame);
    frame.push(checksum);
    frame
}

// ── Checksum (from VPW.cs CalcBlockChecksum / AddBlockChecksum) ───────────

/// J1850 VPW single-byte checksum: sum of all bytes mod 256.
/// Source: VPW.cs CalcBlockChecksum — sums from index 0, wrapping u8.
pub fn vpw_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Validate a received VPW frame checksum.
pub fn validate_frame(frame: &[u8]) -> bool {
    if frame.len() < 2 {
        return false;
    }
    let payload = &frame[..frame.len() - 1];
    let expected = vpw_checksum(payload);
    expected == frame[frame.len() - 1]
}

// ── Response parser ────────────────────────────────────────────────────────

/// Parse a Mode 01 OBD-II response frame and return the data bytes.
/// Expected format: [Hdr x3] [0x41] [PID] [Data...] [Checksum]
/// The 0x41 = 0x40 | 0x01 (positive response to Mode 01 request).
pub fn parse_mode01_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 {
        return None;
    }
    // Header: bytes 0-2 (priority, source, dest)
    // Byte 3: service response (0x41 for Mode 01)
    // Byte 4: PID echo
    // Bytes 5..(len-1): data
    // Last byte: checksum
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_frame(frame) {
        let data = frame[5..frame.len() - 1].to_vec();
        return Some(data);
    }
    None
}

/// Parse a Mode 22 (Read Data by ID) response.
/// Expected: [Hdr x3] [0x62] [DID_H] [DID_L] [Data...] [Checksum]
pub fn parse_mode22_response(frame: &[u8], did_high: u8, did_low: u8) -> Option<Vec<u8>> {
    if frame.len() < 7 {
        return None;
    }
    if frame[3] == 0x62 && frame[4] == did_high && frame[5] == did_low && validate_frame(frame) {
        let data = frame[6..frame.len() - 1].to_vec();
        return Some(data);
    }
    None
}

/// Parse a Mode 43 (DTC response) frame and return raw DTC bytes.
/// Each DTC is 2 bytes. Number of DTCs = (frame.len() - 5) / 2
pub fn parse_dtc_response(frame: &[u8]) -> Option<Vec<[u8; 2]>> {
    if frame.len() < 5 {
        return None;
    }
    if frame[3] != 0x43 {
        return None;
    }
    if !validate_frame(frame) {
        return None;
    }
    let data = &frame[5..frame.len() - 1];
    let dtcs: Vec<[u8; 2]> = data
        .chunks_exact(2)
        .map(|c| [c[0], c[1]])
        .collect();
    Some(dtcs)
}

// ── Serial I/O ─────────────────────────────────────────────────────────────

/// Write a VPW frame to the serial port.
pub fn send_frame(port: &mut Box<dyn SerialPort + Send>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame).map_err(|e| format!("VPW send error: {}", e))
}

/// Read a response frame from the serial port (up to 256 bytes).
pub fn recv_frame(port: &mut Box<dyn SerialPort + Send>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 256];
    let n = port.read(&mut buf).map_err(|e| format!("VPW recv error: {}", e))?;
    if n == 0 {
        return Err("VPW recv: no data".to_string());
    }
    Ok(buf[..n].to_vec())
}

/// Send a request and read one response, with retry on empty reads.
pub fn request_response(
    port: &mut Box<dyn SerialPort + Send>,
    frame: &[u8],
) -> Result<Vec<u8>, String> {
    send_frame(port, frame)?;
    // Allow up to 3 read attempts (bus latency can cause empty first reads)
    for _ in 0..3 {
        match recv_frame(port) {
            Ok(resp) if !resp.is_empty() => return Ok(resp),
            Ok(_) => continue,
            Err(e) if e.contains("no data") => continue,
            Err(e) => return Err(e),
        }
    }
    Err("VPW: no response after 3 attempts".to_string())
}

// ── Flash / Kernel protocol builders (for P01 guided pipeline) ─────────────

/// Build Mode 34 (Request Download) for kernel/cal upload.
/// Format used by many GM kernels: 0x34 [fmt=0] [addr:4B BE] [size:4B BE] + cs
pub fn build_mode34_request(address: u32, size: u32) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x34, 0x00];
    frame.extend_from_slice(&address.to_be_bytes());
    frame.extend_from_slice(&size.to_be_bytes());
    let cs = vpw_checksum(&frame);
    frame.push(cs);
    frame
}

/// Build a Mode 36 (Data Transfer) chunk for kernel or data.
/// Simple: [prio, pcm, tool, 0x36, ...data...] + cs
/// Real loaders may prefix chunk with seq/addr but this matches basic observed.
pub fn build_mode36_chunk(data: &[u8]) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x36];
    frame.extend_from_slice(data);
    let cs = vpw_checksum(&frame);
    frame.push(cs);
    frame
}

/// Build Mode 37 (Request Transfer Exit / start kernel).
pub fn build_mode37_request() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x37];
    let cs = vpw_checksum(&frame);
    frame.push(cs);
    frame
}

/// Convenience full frame builder using physical high prio for direct PCM.
pub fn build_physical_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR];
    frame.extend_from_slice(payload);
    let cs = vpw_checksum(&frame);
    frame.push(cs);
    frame
}
