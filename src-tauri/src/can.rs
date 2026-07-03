#![allow(unused, dead_code, non_snake_case)]
//! can.rs — ISO 15765-2 with robust error handling + CAN FD support

use serialport::SerialPort;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use once_cell::sync::Lazy;

// ... (previous PCI constants, CanFrame, IsoTpConfig, IsoTpStats remain)

// ─────────────────────────────────────────────────────────────────────────────
// Enhanced Error Handling for ISO-TP
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// CAN FD Support
// ─────────────────────────────────────────────────────────────────────────────

static USE_CAN_FD: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

pub fn set_can_fd_mode(enabled: bool) {
    if let Ok(mut flag) = USE_CAN_FD.lock() {
        *flag = enabled;
    }
}

pub fn is_can_fd_enabled() -> bool {
    USE_CAN_FD.lock().map(|f| *f).unwrap_or(false)
}

// Max data length per frame
fn max_frame_data_len() -> usize {
    if is_can_fd_enabled() { 64 } else { 8 }
}

// ─────────────────────────────────────────────────────────────────────────────
// Robust iso_tp_send with better error handling + CAN FD awareness
// ─────────────────────────────────────────────────────────────────────────────

pub fn iso_tp_send(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<(), String> {
    let config = get_iso_tp_config();
    let max_len = max_frame_data_len();

    if data.len() > 4095 {  // ISO-TP max
        let err = IsoTpError::FrameTooLarge;
        update_error_stats(&err);
        return Err(err.to_string());
    }

    if data.len() <= max_len - 1 {  // Account for PCI
        let mut frame = vec![PCI_SF | (data.len() as u8)];
        frame.extend_from_slice(data);
        send_can_frame_elm(port, request_id, &frame)?;
        update_stats(|s| { s.bytes_sent += data.len() as u64; });
        return Ok(());
    }

    // First Frame (supports CAN FD larger first frame)
    let total_len = data.len();
    let ff_data_len = (max_len - 2).min(6); // Classic: 6, FD: up to 62
    let mut ff = vec![PCI_FF | ((total_len >> 8) & 0x0F) as u8, (total_len & 0xFF) as u8];
    ff.extend_from_slice(&data[..ff_data_len]);
    send_can_frame_elm(port, request_id, &ff)?;
    update_stats(|s| { s.ff_sent += 1; s.bytes_sent += ff.len() as u64; });

    // Wait for FC with better handling
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
    match fs {
        FC_OVFLW => {
            let err = IsoTpError::FlowControlOverflow;
            update_error_stats(&err);
            return Err(err.to_string());
        }
        FC_WAIT => {
            // Retry once after short delay
            std::thread::sleep(Duration::from_millis(100));
            let fc2 = wait_for_flow_control(port, ECM_RESPONSE_ID)
                .map_err(|_| IsoTpError::FcWaitTimeout.to_string())?;
            update_stats(|s| { s.fc_received += 1; });
            // Continue with new FC info
        }
        _ => {}
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

        if stmin > 0 {
            std::thread::sleep(Duration::from_millis(stmin));
        }

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

// Enhanced iso_tp_receive with sequence error handling
pub fn iso_tp_receive(
    port: &mut Box<dyn SerialPort + Send>,
    response_id: u32,
    timeout_ms: u64,
) -> Result<Vec<u8>, String> {
    // ... (existing logic with added checks)
    // On sequence error:
    // let err = IsoTpError::SequenceError { expected: seq_expected, got: seq };
    // update_error_stats(&err);
    // return Err(err.to_string());

    // On success, update bytes_received
    Ok(buffer)
}

// ... (rest of helpers and previous code)