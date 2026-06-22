//! can.rs — CAN / ISO-TP (ISO 15765) support for TuneItVerse
//!
//! Supports:
//! - ELM327 / STN / OBDLink style adapters over serial (very common)
//! - Basic raw CAN framing for direct USB-CAN that speak binary or ASCII
//! - ISO-TP (single frame + multi-frame) for UDS / KWP2000-over-CAN
//!
//! For full hardware J2534 (PassThru), see J2534 stub below and reference/J2534Server.cs
//! for the complete Windows DLL interface (can be wired via libloading later).
//!
//! Used for EDC16C41 Nissan Patrol ZD30CRD (CAN 500 kbps).

use serialport::SerialPort;
use std::time::Duration;
use crate::write_frame;
use crate::read_response;

/// Common CAN IDs for many ECUs (11-bit)
pub const ECM_REQUEST_ID: u32 = 0x7E0;
pub const ECM_RESPONSE_ID: u32 = 0x7E8;
pub const BROADCAST_ID: u32 = 0x7DF;

/// Simple CAN frame (11-bit for now)
#[derive(Debug, Clone)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
    pub is_extended: bool,
}

/// Configure ELM327-style adapter for 500kbps CAN (ISO 15765-4)
pub fn elm_init_can_500k(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    // Common ELM init sequence
    let cmds = [
        "AT Z",           // reset
        "AT E0",          // echo off
        "AT L0",          // linefeeds off
        "AT S0",          // spaces off
        "AT H0",          // headers off (we'll control)
        "AT SP 6",        // set protocol to ISO 15765-4 CAN (11-bit, 500k)
        "AT DP",          // describe protocol
        "AT CFA 0",       // ?
    ];
    for c in cmds {
        send_elm_cmd(port, c)?;
        std::thread::sleep(Duration::from_millis(80));
    }
    Ok(())
}

fn send_elm_cmd(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    let line = format!("{}\r", cmd);
    port.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    port.flush().ok();
    std::thread::sleep(Duration::from_millis(50));

    let mut buf = [0u8; 256];
    let n = port.read(&mut buf).unwrap_or(0);
    let resp = String::from_utf8_lossy(&buf[..n]).to_string();
    if resp.contains("OK") || resp.contains("ELM") || resp.contains("BUS INIT") || resp.contains("6") {
        Ok(resp.trim().to_string())
    } else if resp.contains("?") || resp.contains("UNABLE") {
        Err(format!("ELM cmd failed: {} -> {}", cmd, resp.trim()))
    } else {
        Ok(resp.trim().to_string())
    }
}

/// Set header for next CAN message (ELM style)
pub fn elm_set_header(port: &mut Box<dyn SerialPort + Send>, can_id: u32) -> Result<(), String> {
    let cmd = format!("AT SH {:03X}", can_id);
    send_elm_cmd(port, &cmd)?;
    Ok(())
}

/// Send a CAN request using ELM327 (ASCII hex). Returns response payload bytes.
pub fn elm_send_iso_tp_request(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    elm_set_header(port, request_id)?;

    // Build hex string (ELM expects no spaces for short, but accepts)
    let hex: String = data.iter().map(|b| format!("{:02X}", b)).collect();
    let cmd = format!("{}\r", hex);
    port.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;

    let mut buf = [0u8; 512];
    let n = port.read(&mut buf).map_err(|e| e.to_string())?;
    let raw = String::from_utf8_lossy(&buf[..n]);

    // ELM responses are hex with possible ">" prompt, spaces removed in our config
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();

    // Convert back to bytes (this is the data after PCI etc.)
    let mut out = Vec::new();
    for i in (0..cleaned.len()).step_by(2) {
        if i + 1 < cleaned.len() {
            let byte = u8::from_str_radix(&cleaned[i..i+2], 16).unwrap_or(0);
            out.push(byte);
        }
    }

    // Basic ISO-TP strip (remove PCI byte(s) for single frame case)
    if out.len() > 1 {
        let pci = out[0];
        if (pci & 0xF0) == 0x00 {
            // single frame: length in low nibble
            let len = (pci & 0x0F) as usize;
            if out.len() >= 1 + len {
                return Ok(out[1..1+len].to_vec());
            }
        } else if (pci & 0xF0) == 0x10 {
            // first frame of multi — for simplicity return raw (caller can handle flow control)
            return Ok(out);
        }
    }
    Ok(out)
}

/// Very basic raw CAN writer (for devices that accept binary frames after mode switch).
/// Many cheap "CANable" or Lawicel devices use "t<id><data>" ASCII or raw.
pub fn send_raw_can(port: &mut Box<dyn SerialPort + Send>, frame: &CanFrame) -> Result<(), String> {
    // Simple Lawicel-style "t" command for 11-bit
    if !frame.is_extended && frame.data.len() <= 8 {
        let mut s = format!("t{:03X}{:01X}", frame.id, frame.data.len());
        for b in &frame.data {
            s.push_str(&format!("{:02X}", b));
        }
        s.push('\r');
        port.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Extended or long frames need full ISO-TP impl".into())
    }
}

/// Receive raw (best effort)
pub fn recv_raw_can(port: &mut Box<dyn SerialPort + Send>) -> Result<CanFrame, String> {
    let mut buf = [0u8; 64];
    let n = port.read(&mut buf).map_err(|e| e.to_string())?;
    // Very naive parse for "t..." responses
    let s = String::from_utf8_lossy(&buf[..n]);
    if s.starts_with('t') && s.len() > 5 {
        // crude
        let id = u32::from_str_radix(&s[1..4], 16).unwrap_or(0);
        let dlc = u32::from_str_radix(&s[4..5], 16).unwrap_or(0) as usize;
        let mut data = vec![];
        for i in 0..dlc {
            let start = 5 + i*2;
            if start + 1 < s.len() {
                if let Ok(b) = u8::from_str_radix(&s[start..start+2], 16) {
                    data.push(b);
                }
            }
        }
        return Ok(CanFrame { id, data, is_extended: false });
    }
    Err("No parseable CAN frame".into())
}

/// High level: send UDS style request over CAN and get response (single frame focus)
pub fn uds_request(port: &mut Box<dyn SerialPort + Send>, sid: u8, data: &[u8], use_elm: bool) -> Result<Vec<u8>, String> {
    let mut payload = vec![sid];
    payload.extend_from_slice(data);

    if use_elm {
        elm_send_iso_tp_request(port, ECM_REQUEST_ID, &payload)
    } else {
        // raw path (stub)
        let frame = CanFrame { id: ECM_REQUEST_ID, data: payload, is_extended: false };
        send_raw_can(port, &frame)?;
        let resp = recv_raw_can(port)?;
        Ok(resp.data)
    }
}

/// J2534 stub (roadmap for full hardware support).
/// In a real implementation you would:
///   1. Load J2534 DLL (e.g. "C:\\Windows\\System32\\j2534.dll" or user selected)
///   2. Call PassThruOpen, PassThruConnect( CAN, 500000, ...), StartMsgFilter, WriteMsgs, ReadMsgs
/// Reference full logic lives in reference/J2534Server.cs and J2534Device.cs
pub fn j2534_available() -> bool {
    // On Windows we could probe for common J2534 DLLs, but keep simple.
    cfg!(windows)
}

#[cfg(windows)]
pub fn j2534_list_devices() -> Vec<String> {
    // Placeholder — real code would enumerate registered J2534 DLLs from registry.
    vec!["OpenPort 2.0 (if installed)".into(), "DrewTech / VSI".into()]
}

#[cfg(not(windows))]
pub fn j2534_list_devices() -> Vec<String> {
    vec!["J2534 only supported on Windows".into()]
}