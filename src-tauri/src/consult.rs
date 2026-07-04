#![allow(unused, dead_code)]
//! consult.rs — Enhanced Nissan Consult II for ZD30CRD with polling + multi-register support

use serialport::SerialPort;
use std::time::Duration;
use crate::write_frame;
use serde_json::json;

pub const CONSULT_BAUD: u32 = 9600;

// ... (consult_init and consult_send_command remain the same)

/// Read multiple registers in one command (faster for live logging)
pub fn consult_read_registers(port: &mut Box<dyn SerialPort + Send>, regs: &[u8]) -> Result<Vec<u16>, String> {
    if regs.is_empty() { return Ok(vec![]); }
    let mut req = vec![0x5A, regs.len() as u8];
    req.extend_from_slice(regs);
    let resp = consult_send_command(port, 0x5A, &req[1..])?;

    let mut values = vec![];
    for i in 0..regs.len() {
        if resp.len() > i*2 + 1 {
            let val = ((resp[i*2] as u16) << 8) | (resp.get(i*2 + 1).copied().unwrap_or(0) as u16);
            values.push(val);
        } else {
            values.push(0);
        }
    }
    Ok(values)
}

/// ZD30-specific diesel PIDs with realistic scaling
pub fn consult_read_basic_diesel_data(port: &mut Box<dyn SerialPort + Send>) -> Result<serde_json::Value, String> {
    let regs = [0x00u8, 0x0B, 0x23, 0x1C]; // RPM, Boost/MAP, Rail Press, Inj Pulse
    let vals = consult_read_registers(port, &regs)?;

    let rpm = vals.get(0).copied().unwrap_or(0) as f64;
    let boost_raw = vals.get(1).copied().unwrap_or(0) as f64;
    let rail_raw = vals.get(2).copied().unwrap_or(0) as f64;
    let inj_raw = vals.get(3).copied().unwrap_or(0) as f64;

    // Typical ZD30 scaling (adjust per your bin)
    let boost_kpa = boost_raw * 0.5;           // example
    let rail_pressure_bar = rail_raw * 0.1;
    let egr_duty = (inj_raw % 100.0) as f64;   // placeholder
    let injection_ms = inj_raw * 0.01;

    Ok(json!({
        "rpm": rpm,
        "boost_kpa": boost_kpa,
        "rail_pressure_bar": rail_pressure_bar,
        "egr_duty_percent": egr_duty,
        "injection_duration_ms": injection_ms
    }))
}

/// NEW: Continuous polling loop for live data logging
pub fn consult_poll_loop(
    port: &mut Box<dyn SerialPort + Send>,
    interval_ms: u64,
    mut callback: impl FnMut(serde_json::Value),
) -> Result<(), String> {
    loop {
        match consult_read_basic_diesel_data(port) {
            Ok(data) => callback(data),
            Err(e) => {
                eprintln!("[Consult] Poll error: {}. Attempting recovery...", e);
                let _ = consult_init(port);
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
}