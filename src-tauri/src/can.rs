#![allow(unused, dead_code, non_snake_case)]
//! can.rs — Professional ISO 15765-2 (ISO-TP) with Config + Stats

use serialport::SerialPort;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use once_cell::sync::Lazy;

// ... (previous constants and CanFrame remain)

// ─────────────────────────────────────────────────────────────────────────────
// ISO-TP Configuration (now exposed)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct IsoTpConfig {
    pub block_size: u8,      // 0 = unlimited
    pub stmin_ms: u64,       // Separation time between CFs
}

impl Default for IsoTpConfig {
    fn default() -> Self {
        Self {
            block_size: 0,     // Unlimited (good for most modern ECUs)
            stmin_ms: 5,       // Safe default for ELM327 / OBDLink
        }
    }
}

static ISO_TP_CONFIG: Lazy<Mutex<IsoTpConfig>> = Lazy::new(|| Mutex::new(IsoTpConfig::default()));

pub fn set_iso_tp_config(block_size: u8, stmin_ms: u64) {
    if let Ok(mut cfg) = ISO_TP_CONFIG.lock() {
        cfg.block_size = block_size;
        cfg.stmin_ms = stmin_ms;
    }
}

pub fn get_iso_tp_config() -> IsoTpConfig {
    ISO_TP_CONFIG.lock().map(|c| *c).unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// ISO-TP Statistics & Debugging
// ─────────────────────────────────────────────────────────────────────────────

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
    pub avg_stmin_used_ms: f64,
}

static ISO_TP_STATS: Lazy<Mutex<IsoTpStats>> = Lazy::new(|| Mutex::new(IsoTpStats::default()));

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

// ─────────────────────────────────────────────────────────────────────────────
// Enhanced ISO-TP Send with Config + Stats
// ─────────────────────────────────────────────────────────────────────────────

pub fn iso_tp_send(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<(), String> {
    let config = get_iso_tp_config();

    if data.len() <= 7 {
        let mut frame = vec![(PCI_SF | data.len() as u8)];
        frame.extend_from_slice(data);
        send_can_frame_elm(port, request_id, &frame)?;
        update_stats(|s| { s.bytes_sent += data.len() as u64; });
        return Ok(());
    }

    // First Frame
    let total_len = data.len();
    let mut ff = vec![PCI_FF | ((total_len >> 8) & 0x0F) as u8, (total_len & 0xFF) as u8];
    ff.extend_from_slice(&data[..6.min(total_len)]);
    send_can_frame_elm(port, request_id, &ff)?;
    update_stats(|s| { s.ff_sent += 1; s.bytes_sent += ff.len() as u64; });

    // Wait for FC
    let fc = wait_for_flow_control(port, ECM_RESPONSE_ID)?;
    update_stats(|s| { s.fc_received += 1; });

    let fs = fc[0] & 0x0F;
    if fs == FC_OVFLW { 
        update_stats(|s| { s.errors += 1; s.last_error = Some("FC Overflow".into()); });
        return Err("Flow Control Overflow".into()); 
    }

    let block_size = if config.block_size > 0 { config.block_size as usize } else { 
        if fc.len() > 1 { fc[1] as usize } else { 0 } 
    };
    let stmin = config.stmin_ms;

    // Consecutive Frames
    let mut seq = 1u8;
    let mut offset = 6;
    let mut frames_in_block = 0;

    while offset < total_len {
        let mut cf = vec![PCI_CF | seq];
        let chunk_size = 7.min(total_len - offset);
        cf.extend_from_slice(&data[offset..offset+chunk_size]);
        send_can_frame_elm(port, request_id, &cf)?;

        update_stats(|s| { s.cf_sent += 1; s.bytes_sent += cf.len() as u64; });

        offset += chunk_size;
        seq = if seq == 15 { 1 } else { seq + 1 };
        frames_in_block += 1;

        if stmin > 0 {
            std::thread::sleep(Duration::from_millis(stmin));
        }

        if block_size > 0 && frames_in_block >= block_size && offset < total_len {
            let _ = wait_for_flow_control(port, ECM_RESPONSE_ID)?;
            frames_in_block = 0;
            update_stats(|s| { s.fc_received += 1; });
        }
    }

    Ok(())
}

// Enhanced receive with stats
pub fn iso_tp_receive(...) -> Result<Vec<u8>, String> {
    // ... (existing receive logic + update_stats calls for ff_received, cf_received, bytes_received)
    // Add after successful reassembly:
    update_stats(|s| { s.bytes_received += buffer.len() as u64; });
    Ok(buffer)
}

// ... (rest of file with helpers unchanged)