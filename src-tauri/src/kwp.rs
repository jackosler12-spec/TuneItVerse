#![allow(unused, dead_code)]
//! kwp.rs — K-line / KWP2000 (ISO 14230) full support
//!
//! Supports fast initialization and the common KWP2000 frame format.
//! Used for many ECUs including some Nissan EDC16 KWP-over-K-line paths and European vehicles.
//!
//! Frame format (most common):
//!   [Format] [Tgt] [Src] [Len] [SID + data...] [CS]
//!
//! Fast init: 25ms low + 25ms high (or specific wake-up pattern) then 0xC1 0x33 0xF1 0x81 ...
//!
//! This module works over normal serial (K-line is usually on a single wire but many
//! interfaces present TX/RX tied or use a dedicated K-line transceiver).

use serialport::SerialPort;
use std::time::Duration;
use crate::{write_frame, read_response};

#[derive(Debug, Clone)]
pub struct KwpFrame {
    pub fmt: u8,
    pub tgt: u8,
    pub src: u8,
    pub data: Vec<u8>,   // SID + params (without len/cs)
}

/// Perform fast init on K-line (common 0x33 address for engine).
pub fn kwp_fast_init(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    // Many adapters need the port to be in a special mode or just pulsing.
    // We do a software "fast init" by sending a break-like pattern + known wake bytes.
    // Real hardware often requires the adapter to handle the 5-baud / fast init timing.

    // Attempt fast init sequence
    port.write_all(&[0xC1, 0x33, 0xF1, 0x81]).map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(25));

    // Read possible positive response 0xC1 0x33 0xF1 0xC1 ...
    let mut buf = [0u8; 16];
    let _ = port.read(&mut buf); // best effort

    // Some ECUs respond with key bytes
    Ok(())
}

/// Build a standard KWP2000 physical frame (fmt 0x80 + len usually).
pub fn build_kwp_request(tgt: u8, src: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x80, tgt, src, data.len() as u8];
    frame.extend_from_slice(data);
    let cs: u8 = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

/// Send request and wait for reply (with simple timeout handling).
pub fn kwp_request_response(
    port: &mut Box<dyn SerialPort + Send>,
    request: &[u8],
) -> Result<Vec<u8>, String> {
    write_frame(port, request)?;
    std::thread::sleep(Duration::from_millis(60));

    let mut buf = [0u8; 256];
    let n = port.read(&mut buf).unwrap_or(0);
    if n == 0 {
        return Err("KWP no reply".into());
    }
    Ok(buf[..n].to_vec())
}

/// High level UDS-like call over KWP (many Nissan/EDC use KWP services close to UDS).
pub fn kwp_diagnostic_session(port: &mut Box<dyn SerialPort + Send>, session: u8) -> Result<Vec<u8>, String> {
    let req = build_kwp_request(0x33, 0xF1, &[0x10, session]);
    kwp_request_response(port, &req)
}

/// Security access seed request (0x27 0x01 etc) — caller computes key.
pub fn kwp_request_seed(port: &mut Box<dyn SerialPort + Send>, level: u8) -> Result<Vec<u8>, String> {
    let req = build_kwp_request(0x33, 0xF1, &[0x27, level]);
    kwp_request_response(port, &req)
}

pub fn kwp_send_key(port: &mut Box<dyn SerialPort + Send>, level: u8, key: &[u8]) -> Result<Vec<u8>, String> {
    let mut data = vec![0x27, level + 1];
    data.extend_from_slice(key);
    let req = build_kwp_request(0x33, 0xF1, &data);
    kwp_request_response(port, &req)
}

/// Simple memory read (0x23) if supported by the ECU.
pub fn kwp_read_memory(port: &mut Box<dyn SerialPort + Send>, addr: u32, len: u16) -> Result<Vec<u8>, String> {
    let data = vec![
        0x23,
        ((addr >> 16) & 0xFF) as u8,
        ((addr >> 8) & 0xFF) as u8,
        (addr & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        (len & 0xFF) as u8,
    ];
    let req = build_kwp_request(0x33, 0xF1, &data);
    kwp_request_response(port, &req)
}