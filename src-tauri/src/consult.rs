#![allow(unused, dead_code)]
//! consult.rs — Nissan Consult II / Consult protocol for ZD30CRD and similar
//!
//! Nissan Patrol GU ZD30CRD (EDC16C41) often supports both CAN and the older Consult port.
//! Consult II is a Nissan-proprietary serial protocol at 9600 baud 8N1 (sometimes 38400 on later).
//!
//! Frame style (typical observed from community + reference tools):
//!   Many commands are sent as 0x5A <subcmd> <register or data>
//!   Responses come back with echoed command + data.
//!
//! This module provides basic init + register read + raw command for the user's ZD30CRD.
//! Full memory access / flashing usually moves to CAN/UDS on these ECUs, but Consult is great
//! for live data and some older functions.

use serialport::SerialPort;
use std::time::Duration;
use crate::write_frame;

/// Common Consult baud for most Nissan ECUs
pub const CONSULT_BAUD: u32 = 9600;

/// Basic init / wake for Consult port.
/// Many devices just need to be opened at 9600 and a couple of commands sent.
pub fn consult_init(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    // Some implementations send 0xFF or a specific wake.
    let wake = [0xFFu8];
    let _ = write_frame(port, &wake);
    std::thread::sleep(Duration::from_millis(100));

    // Try a harmless read (ECU ID or something)
    let _ = consult_send_command(port, 0xD0, &[]); // many firmwares accept
    Ok(())
}

/// Send a Consult command and return raw response.
/// Format is very device specific; this is the low-level primitive.
pub fn consult_send_command(
    port: &mut Box<dyn SerialPort + Send>,
    cmd: u8,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let mut frame = vec![cmd];
    frame.extend_from_slice(data);
    // Some variants add length or terminator. We keep simple.
    write_frame(port, &frame)?;
    std::thread::sleep(Duration::from_millis(40));

    let mut buf = [0u8; 128];
    let n = port.read(&mut buf).unwrap_or(0);
    Ok(buf[..n].to_vec())
}

/// Read a "register" using the common 0x5A style (very common in Nissan data logging).
/// register: the known Nissan consult register address (from community lists or WinOLS).
pub fn consult_read_register(port: &mut Box<dyn SerialPort + Send>, reg: u8) -> Result<u16, String> {
    // Typical: 0x5A 0x01 0x<reg>   or 0x5A <len> <reg>
    let req = vec![0x5A, 0x01, reg];
    let resp = consult_send_command(port, 0x5A, &req[1..])?;

    // Response often echoes and then gives 1 or 2 bytes of data.
    if resp.len() >= 2 {
        // crude big-endian or direct
        let val = ((resp[0] as u16) << 8) | (resp.get(1).copied().unwrap_or(0) as u16);
        return Ok(val);
    }
    // Fallback: return first byte as value
    Ok(resp.get(0).copied().unwrap_or(0) as u16)
}

/// Common ZD30 / diesel registers (examples — real values come from your bin or logging software)
pub const REG_RPM: u8 = 0x00;        // typical
pub const REG_BOOST: u8 = 0x0B;      // or MAP
pub const REG_MAF: u8 = 0x0C;
pub const REG_INJ_PULSE: u8 = 0x1C;
pub const REG_RAIL_PRESS: u8 = 0x23;

/// Convenience: read a few key diesel params.
pub fn consult_read_basic_diesel_data(port: &mut Box<dyn SerialPort + Send>) -> Result<serde_json::Value, String> {
    let rpm = consult_read_register(port, REG_RPM).unwrap_or(0);
    let boost_raw = consult_read_register(port, REG_BOOST).unwrap_or(0);
    let maf = consult_read_register(port, REG_MAF).unwrap_or(0);

    Ok(serde_json::json!({
        "rpm": rpm,
        "boost_raw": boost_raw,
        "maf_raw": maf,
        "note": "Scale factors depend on exact ZD30 calibration. Cross reference with your bin or TunerPro/Consult logger defs."
    }))
}

// For full flash / memory on these ECUs you will usually switch to the CAN/UDS path (see can.rs + DB).
// Consult is excellent for live data and some actuator tests.
