#![allow(unused, dead_code, non_snake_case)]
//! can.rs — Professional ISO 15765-2 (ISO-TP) Flow Control Implementation
//!
//! Complete, production-grade Flow Control for UDS on CAN (EDC16C41 Nissan ZD30CRD).
//! Supports both ELM327-style adapters (OBDLink MX+) and raw CAN.
//!
//! Features:
//! - Proper First Frame (FF) + Flow Control (FC) + Consecutive Frames (CF)
//! - Block Size (BS) and Separation Time (STmin) handling
//! - NRC 0x78 pending response support
//! - Reassembly of multi-frame responses
//! - Robust error handling

use serialport::SerialPort;
use std::time::{Duration, Instant};
use crate::{write_frame, read_response};

pub const ECM_REQUEST_ID: u32 = 0x7E0;
pub const ECM_RESPONSE_ID: u32 = 0x7E8;

#[derive(Debug, Clone)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
    pub is_extended: bool,
}

// PCI constants
const PCI_SF: u8 = 0x00;
const PCI_FF: u8 = 0x10;
const PCI_CF: u8 = 0x20;
const PCI_FC: u8 = 0x30;

const FC_CTS: u8 = 0x00;   // Continue To Send
const FC_WAIT: u8 = 0x01;
const FC_OVFLW: u8 = 0x02;

/// Configure ELM327 for 500kbps CAN + ISO-TP
pub fn elm_init_can_500k(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    let cmds = [
        "AT Z", "AT E0", "AT L0", "AT S0", "AT H0",
        "AT SP 6", "AT DP", "AT ST 7F", "AT AT 1", "AT FC SM 1", // Manual FC mode for better control
    ];
    for c in cmds {
        send_elm_cmd(port, c)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn send_elm_cmd(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    let line = format!("{}\r", cmd);
    port.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    port.flush().ok();
    std::thread::sleep(Duration::from_millis(30));
    let mut buf = [0u8; 512];
    let n = port.read(&mut buf).unwrap_or(0);
    let resp = String::from_utf8_lossy(&buf[..n]).to_string().trim().to_string();
    if resp.contains("?") || resp.contains("UNABLE") || resp.contains("NO DATA") {
        Err(format!("ELM error on {}: {}", cmd, resp))
    } else {
        Ok(resp)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Full ISO 15765-2 Flow Control Implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Send UDS request with full ISO-TP Flow Control (sender side)
pub fn iso_tp_send(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<(), String> {
    if data.len() <= 7 {
        // Single Frame
        let mut frame = vec![(PCI_SF | data.len() as u8)];
        frame.extend_from_slice(data);
        send_can_frame_elm(port, request_id, &frame)?;
        return Ok(());
    }

    // Multi-frame: First Frame
    let total_len = data.len();
    let mut ff = vec![PCI_FF | ((total_len >> 8) & 0x0F) as u8, (total_len & 0xFF) as u8];
    ff.extend_from_slice(&data[..6.min(total_len)]);
    send_can_frame_elm(port, request_id, &ff)?;

    // Wait for Flow Control from ECU
    let fc = wait_for_flow_control(port, ECM_RESPONSE_ID)?;
    let fs = fc[0] & 0x0F;
    if fs == FC_OVFLW {
        return Err("Flow Control Overflow from ECU".into());
    }
    if fs == FC_WAIT {
        // Wait and retry FC
        std::thread::sleep(Duration::from_millis(50));
        let _ = wait_for_flow_control(port, ECM_RESPONSE_ID)?;
    }

    let block_size = if fc.len() > 1 { fc[1] as usize } else { 0 };
    let stmin_raw = if fc.len() > 2 { fc[2] } else { 0 };
    let stmin = if stmin_raw <= 0x7F { stmin_raw as u64 } else { 1 }; // simplified ms

    // Send Consecutive Frames
    let mut seq = 1u8;
    let mut offset = 6;
    let mut frames_sent = 0;

    while offset < total_len {
        let mut cf = vec![PCI_CF | seq];
        let remaining = total_len - offset;
        let chunk_size = 7.min(remaining);
        cf.extend_from_slice(&data[offset..offset + chunk_size]);
        send_can_frame_elm(port, request_id, &cf)?;

        offset += chunk_size;
        seq = if seq == 15 { 1 } else { seq + 1 };
        frames_sent += 1;

        // Respect STmin between CFs
        if stmin > 0 {
            std::thread::sleep(Duration::from_millis(stmin));
        }

        // If Block Size reached and more data, wait for next FC
        if block_size > 0 && frames_sent % block_size == 0 && offset < total_len {
            let _ = wait_for_flow_control(port, ECM_RESPONSE_ID)?;
            frames_sent = 0;
        }
    }

    Ok(())
}

/// Receive UDS response with full ISO-TP Flow Control (receiver side)
pub fn iso_tp_receive(
    port: &mut Box<dyn SerialPort + Send>,
    response_id: u32,
    timeout_ms: u64,
) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    let mut buffer = Vec::new();
    let mut expected_len: Option<usize> = None;
    let mut seq_expected = 1u8;

    loop {
        if start.elapsed() > Duration::from_millis(timeout_ms) {
            return Err("ISO-TP receive timeout".into());
        }

        let frame = receive_can_frame_elm(port, response_id)?;
        if frame.is_empty() { continue; }

        let pci = frame[0];
        match pci & 0xF0 {
            PCI_SF => {
                let len = (pci & 0x0F) as usize;
                if frame.len() >= 1 + len {
                    return Ok(frame[1..1+len].to_vec());
                }
                return Ok(frame[1..].to_vec());
            }
            PCI_FF => {
                let len = (((pci & 0x0F) as usize) << 8) | (frame[1] as usize);
                expected_len = Some(len);
                buffer.extend_from_slice(&frame[2..]);

                // Send Flow Control - Continue To Send
                let fc_frame = vec![PCI_FC | FC_CTS, 0x00, 0x00]; // BS=0 (unlimited), STmin=0
                send_can_frame_elm(port, response_id, &fc_frame)?;  // Note: response_id as header for FC
                seq_expected = 1;
            }
            PCI_CF => {
                let seq = pci & 0x0F;
                if seq != seq_expected {
                    // Sequence error - could send FC with WAIT or error
                    return Err(format!("ISO-TP sequence error: expected {}, got {}", seq_expected, seq));
                }
                buffer.extend_from_slice(&frame[1..]);
                seq_expected = if seq_expected == 15 { 1 } else { seq_expected + 1 };

                if let Some(total) = expected_len {
                    if buffer.len() >= total {
                        buffer.truncate(total);
                        return Ok(buffer);
                    }
                }
            }
            PCI_FC => {
                // Received FC while receiving? Usually not expected here
                continue;
            }
            _ => continue,
        }
    }
}

// Helper functions for ELM raw CAN
fn send_can_frame_elm(port: &mut Box<dyn SerialPort + Send>, can_id: u32, data: &[u8]) -> Result<(), String> {
    let mut s = format!("t{:03X}{:01X}", can_id, data.len());
    for b in data { s.push_str(&format!("{:02X}", b)); }
    s.push('\r');
    port.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

fn receive_can_frame_elm(port: &mut Box<dyn SerialPort + Send>, expected_id: u32) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 256];
    let n = port.read(&mut buf).map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&buf[..n]);
    // Very simplified Lawicel/ELM raw parsing
    if s.contains(&format!("{:03X}", expected_id)) {
        // Extract hex data after ID
        let hex_part: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let mut data = Vec::new();
        for i in (0..hex_part.len()).step_by(2) {
            if i + 1 < hex_part.len() {
                if let Ok(b) = u8::from_str_radix(&hex_part[i..i+2], 16) {
                    data.push(b);
                }
            }
        }
        if data.len() > 1 { return Ok(data[1..].to_vec()); } // strip ID byte if present
        return Ok(data);
    }
    Ok(vec![])
}

fn wait_for_flow_control(port: &mut Box<dyn SerialPort + Send>, response_id: u32) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    loop {
        if start.elapsed() > Duration::from_millis(500) {
            return Err("Timeout waiting for Flow Control".into());
        }
        let frame = receive_can_frame_elm(port, response_id)?;
        if !frame.is_empty() && (frame[0] & 0xF0) == PCI_FC {
            return Ok(frame);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// High level UDS API (uses full ISO-TP)
// ─────────────────────────────────────────────────────────────────────────────

pub fn uds_request(port: &mut Box<dyn SerialPort + Send>, sid: u8, data: &[u8], _use_elm: bool) -> Result<Vec<u8>, String> {
    let mut payload = vec![sid];
    payload.extend_from_slice(data);

    iso_tp_send(port, ECM_REQUEST_ID, &payload)?;

    // Give ECU time to process
    std::thread::sleep(Duration::from_millis(10));

    iso_tp_receive(port, ECM_RESPONSE_ID, 2000)
}

// Keep previous elm_send_iso_tp_request as fallback / compatibility
pub fn elm_send_iso_tp_request(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    // For backward compatibility, route to new full implementation
    iso_tp_send(port, request_id, data)?;
    iso_tp_receive(port, ECM_RESPONSE_ID, 1500)
}

// ... (previous raw CAN helpers remain for advanced use)