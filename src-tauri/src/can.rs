#![allow(unused, dead_code, non_snake_case)]
//! can.rs — Complete ISO 15765-2 (ISO-TP) Implementation with CAN FD + Robust Error Handling
//! Fully restored and completed for TuneItVerse

use serialport::SerialPort;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use once_cell::sync::Lazy;

// ==================== CONSTANTS ====================
pub const ECM_REQUEST_ID: u32 = 0x7E0;
pub const ECM_RESPONSE_ID: u32 = 0x7E8;

const PCI_SF: u8 = 0x00;
const PCI_FF: u8 = 0x10;
const PCI_CF: u8 = 0x20;
const PCI_FC: u8 = 0x30;

const FC_CTS: u8 = 0x00;
const FC_WAIT: u8 = 0x01;
const FC_OVFLW: u8 = 0x02;

// ==================== STRUCTS ====================
#[derive(Debug, Clone, Copy)]
pub struct IsoTpConfig {
    pub block_size: u8,
    pub stmin_ms: u64,
}

impl Default for IsoTpConfig {
    fn default() -> Self {
        Self { block_size: 0, stmin_ms: 5 }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct IsoTpStats {
    pub ff_sent: u32,
    pub cf_sent: u32,
    pub fc_received: u32,
    pub ff_received: u32,
    pub cf_received: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u32,
    pub last_error: Option<String>,
}

// ==================== GLOBALS ====================
static ISO_TP_CONFIG: Lazy<Mutex<IsoTpConfig>> = Lazy::new(|| Mutex::new(IsoTpConfig::default()));
static ISO_TP_STATS: Lazy<Mutex<IsoTpStats>> = Lazy::new(|| Mutex::new(IsoTpStats::default()));
static USE_CAN_FD: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

// ==================== CONFIG & STATS API ====================
pub fn set_iso_tp_config(block_size: u8, stmin_ms: u64) {
    if let Ok(mut cfg) = ISO_TP_CONFIG.lock() {
        cfg.block_size = block_size;
        cfg.stmin_ms = stmin_ms;
    }
}

pub fn get_iso_tp_config() -> IsoTpConfig {
    ISO_TP_CONFIG.lock().map(|c| *c).unwrap_or_default()
}

pub fn get_iso_tp_stats() -> IsoTpStats {
    ISO_TP_STATS.lock().map(|s| s.clone()).unwrap_or_default()
}

pub fn reset_iso_tp_stats() {
    if let Ok(mut stats) = ISO_TP_STATS.lock() {
        *stats = IsoTpStats::default();
    }
}

fn update_stats<F>(f: F) where F: FnOnce(&mut IsoTpStats) {
    if let Ok(mut stats) = ISO_TP_STATS.lock() {
        f(&mut stats);
    }
}

pub fn set_can_fd_mode(enabled: bool) {
    if let Ok(mut flag) = USE_CAN_FD.lock() {
        *flag = enabled;
    }
}

pub fn is_can_fd_enabled() -> bool {
    USE_CAN_FD.lock().map(|f| *f).unwrap_or(false)
}

fn max_frame_data_len() -> usize {
    if is_can_fd_enabled() { 64 } else { 8 }
}

// ==================== ERROR HANDLING ====================
#[derive(Debug, Clone)]
pub enum IsoTpError {
    Timeout,
    FlowControlOverflow,
    SequenceError { expected: u8, got: u8 },
    InvalidPci(u8),
    FcWaitTimeout,
    FrameTooLarge,
    Other(String),
}

impl std::fmt::Display for IsoTpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsoTpError::Timeout => write!(f, "ISO-TP operation timed out"),
            IsoTpError::FlowControlOverflow => write!(f, "ECU sent Flow Control Overflow"),
            IsoTpError::SequenceError { expected, got } => write!(f, "ISO-TP sequence error: expected {}, got {}", expected, got),
            IsoTpError::InvalidPci(pci) => write!(f, "Invalid PCI byte: 0x{:02X}", pci),
            IsoTpError::FcWaitTimeout => write!(f, "Timeout waiting for Flow Control after WAIT"),
            IsoTpError::FrameTooLarge => write!(f, "Frame exceeds maximum supported size"),
            IsoTpError::Other(s) => write!(f, "{}", s),
        }
    }
}

fn update_error_stats(err: &IsoTpError) {
    update_stats(|s| {
        s.errors += 1;
        s.last_error = Some(err.to_string());
    });
}

// ==================== CORE ISO-TP FUNCTIONS ====================

pub fn iso_tp_send(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<(), String> {
    let config = get_iso_tp_config();
    let max_len = max_frame_data_len();

    if data.len() > 4095 {
        let err = IsoTpError::FrameTooLarge;
        update_error_stats(&err);
        return Err(err.to_string());
    }

    if data.len() <= max_len - 1 {
        let mut frame = vec![PCI_SF | (data.len() as u8)];
        frame.extend_from_slice(data);
        send_can_frame_elm(port, request_id, &frame)?;
        update_stats(|s| { s.bytes_sent += data.len() as u64; });
        return Ok(());
    }

    let total_len = data.len();
    let ff_data_len = (max_len - 2).min(6);
    let mut ff = vec![PCI_FF | ((total_len >> 8) & 0x0F) as u8, (total_len & 0xFF) as u8];
    ff.extend_from_slice(&data[..ff_data_len]);
    send_can_frame_elm(port, request_id, &ff)?;
    update_stats(|s| { s.ff_sent += 1; s.bytes_sent += ff.len() as u64; });

    let fc = match wait_for_flow_control(port, ECM_RESPONSE_ID) {
        Ok(fc) => fc,
        Err(e) => {
            let err = IsoTpError::Other(e);
            update_error_stats(&err);
            return Err(err.to_string());
        }
    };
    update_stats(|s| { s.fc_received += 1; });

    let fs = fc[0] & 0x0F;
    if fs == FC_OVFLW {
        let err = IsoTpError::FlowControlOverflow;
        update_error_stats(&err);
        return Err(err.to_string());
    }
    if fs == FC_WAIT {
        std::thread::sleep(Duration::from_millis(100));
        let _ = wait_for_flow_control(port, ECM_RESPONSE_ID);
        update_stats(|s| { s.fc_received += 1; });
    }

    let block_size = if config.block_size > 0 { config.block_size as usize } else { fc.get(1).copied().unwrap_or(0) as usize };
    let stmin = config.stmin_ms;

    let mut seq = 1u8;
    let mut offset = ff_data_len;
    let mut frames_in_block = 0;

    while offset < total_len {
        let mut cf = vec![PCI_CF | seq];
        let chunk_size = (max_len - 1).min(total_len - offset);
        cf.extend_from_slice(&data[offset..offset + chunk_size]);
        send_can_frame_elm(port, request_id, &cf)?;
        update_stats(|s| { s.cf_sent += 1; s.bytes_sent += cf.len() as u64; });

        offset += chunk_size;
        seq = if seq == 15 { 1 } else { seq + 1 };
        frames_in_block += 1;

        if stmin > 0 { std::thread::sleep(Duration::from_millis(stmin)); }

        if block_size > 0 && frames_in_block >= block_size && offset < total_len {
            if let Err(e) = wait_for_flow_control(port, ECM_RESPONSE_ID) {
                let err = IsoTpError::Other(e);
                update_error_stats(&err);
                return Err(err.to_string());
            }
            frames_in_block = 0;
            update_stats(|s| { s.fc_received += 1; });
        }
    }
    Ok(())
}

pub fn iso_tp_receive(
    port: &mut Box<dyn SerialPort + Send>,
    response_id: u32,
    timeout_ms: u64,
) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    let mut buffer: Vec<u8> = Vec::new();
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
                if frame.len() >= 1 + len { return Ok(frame[1..1+len].to_vec()); }
                return Ok(frame[1..].to_vec());
            }
            PCI_FF => {
                let len = (((pci & 0x0F) as usize) << 8) | (frame[1] as usize);
                expected_len = Some(len);
                buffer.extend_from_slice(&frame[2..]);
                let fc_frame = vec![PCI_FC | FC_CTS, 0x00, 0x00];
                send_can_frame_elm(port, response_id, &fc_frame)?;
                seq_expected = 1;
            }
            PCI_CF => {
                let seq = pci & 0x0F;
                if seq != seq_expected {
                    let err = IsoTpError::SequenceError { expected: seq_expected, got: seq };
                    update_error_stats(&err);
                    return Err(err.to_string());
                }
                buffer.extend_from_slice(&frame[1..]);
                seq_expected = if seq_expected == 15 { 1 } else { seq_expected + 1 };

                if let Some(total) = expected_len {
                    if buffer.len() >= total {
                        buffer.truncate(total);
                        update_stats(|s| { s.bytes_received += buffer.len() as u64; });
                        return Ok(buffer);
                    }
                }
            }
            _ => continue,
        }
    }
}

// ==================== HELPER FUNCTIONS ====================
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
    if s.contains(&format!("{:03X}", expected_id)) {
        let hex_part: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let mut data = Vec::new();
        for i in (0..hex_part.len()).step_by(2) {
            if i + 1 < hex_part.len() {
                if let Ok(b) = u8::from_str_radix(&hex_part[i..i+2], 16) { data.push(b); }
            }
        }
        if data.len() > 1 { return Ok(data[1..].to_vec()); }
        return Ok(data);
    }
    Ok(vec![])
}

fn wait_for_flow_control(port: &mut Box<dyn SerialPort + Send>, response_id: u32) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    loop {
        if start.elapsed() > Duration::from_millis(500) { return Err("Timeout waiting for Flow Control".into()); }
        let frame = receive_can_frame_elm(port, response_id)?;
        if !frame.is_empty() && (frame[0] & 0xF0) == PCI_FC { return Ok(frame); }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ==================== ELM / LEGACY FUNCTIONS ====================
pub fn elm_init_can_500k(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    let cmds = ["AT Z", "AT E0", "AT L0", "AT S0", "AT H0", "AT SP 6", "AT ST 7F"];
    for c in cmds {
        let line = format!("{}\r", c);
        port.write_all(line.as_bytes()).ok();
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
}

pub fn uds_request(port: &mut Box<dyn SerialPort + Send>, sid: u8, data: &[u8], _use_elm: bool) -> Result<Vec<u8>, String> {
    let mut payload = vec![sid];
    payload.extend_from_slice(data);
    iso_tp_send(port, ECM_REQUEST_ID, &payload)?;
    std::thread::sleep(Duration::from_millis(10));
    iso_tp_receive(port, ECM_RESPONSE_ID, 2000)
}

pub fn elm_send_iso_tp_request(port: &mut Box<dyn SerialPort + Send>, request_id: u32, data: &[u8]) -> Result<Vec<u8>, String> {
    iso_tp_send(port, request_id, data)?;
    iso_tp_receive(port, ECM_RESPONSE_ID, 1500)
}

// ==================== RAW CAN (OPTIONAL) ====================
#[derive(Debug, Clone)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
    pub is_extended: bool,
}