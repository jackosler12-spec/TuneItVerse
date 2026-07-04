#![allow(unused, dead_code)]
//! kwp.rs — Enhanced KWP2000 with NRC 0x78 retry, P2 timeout, and validation

use serialport::SerialPort;
use std::time::{Duration, Instant};
use crate::{write_frame, read_response};

// ... (KwpFrame and kwp_fast_init remain)

/// Build KWP request (with length validation)
pub fn build_kwp_request(tgt: u8, src: u8, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() > 255 {
        return Err("KWP data too long (max 255 bytes)".into());
    }
    let mut frame = vec![0x80, tgt, src, data.len() as u8];
    frame.extend_from_slice(data);
    let cs: u8 = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    Ok(frame)
}

/// Enhanced request/response with NRC 0x78 retry and length validation
pub fn kwp_request_response(
    port: &mut Box<dyn SerialPort + Send>,
    request: &[u8],
    timeout_ms: u64,
) -> Result<Vec<u8>, String> {
    write_frame(port, request)?;

    let start = Instant::now();
    let mut attempts = 0;

    while start.elapsed() < Duration::from_millis(timeout_ms) && attempts < 5 {
        std::thread::sleep(Duration::from_millis(20));

        let mut buf = [0u8; 512];
        let n = port.read(&mut buf).unwrap_or(0);
        if n == 0 { continue; }

        let resp = &buf[..n];

        // Basic length validation against Len byte
        if resp.len() > 3 {
            let declared_len = resp[3] as usize;
            if resp.len() < 4 + declared_len + 1 {
                // incomplete frame, keep trying
                attempts += 1;
                continue;
            }
        }

        // Check for NRC 0x78 (Request Correctly Received - Response Pending)
        if resp.len() > 5 && resp[4] == 0x7F && resp[5] == 0x78 {
            std::thread::sleep(Duration::from_millis(50)); // wait and retry
            attempts += 1;
            continue;
        }

        return Ok(resp.to_vec());
    }

    Err("KWP request timeout or no valid response".into())
}

// Updated high-level functions to use new kwp_request_response with timeout
pub fn kwp_diagnostic_session(port: &mut Box<dyn SerialPort + Send>, session: u8) -> Result<Vec<u8>, String> {
    let req = build_kwp_request(0x33, 0xF1, &[0x10, session])?;
    kwp_request_response(port, &req, 800)
}

pub fn kwp_request_seed(port: &mut Box<dyn SerialPort + Send>, level: u8) -> Result<Vec<u8>, String> {
    let req = build_kwp_request(0x33, 0xF1, &[0x27, level])?;
    kwp_request_response(port, &req, 600)
}

pub fn kwp_send_key(port: &mut Box<dyn SerialPort + Send>, level: u8, key: &[u8]) -> Result<Vec<u8>, String> {
    let mut data = vec![0x27, level + 1];
    data.extend_from_slice(key);
    let req = build_kwp_request(0x33, 0xF1, &data)?;
    kwp_request_response(port, &req, 600)
}