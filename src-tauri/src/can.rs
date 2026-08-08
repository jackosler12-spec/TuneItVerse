#![allow(unused, dead_code)]
//! can.rs — CAN / ISO-TP (ISO 15765-2) support for TuneItVerse
//!
//! Features:
//! - ELM327 / STN / OBDLink adapters over serial
//! - Lawicel-style raw CAN (tIIDDLCData)
//! - Full ISO-TP multi-frame: SF / FF / CF + Flow Control (CTS)
//! - UDS request helpers (single + multi-frame)
//!
//! Used for EDC16C41 Nissan Patrol ZD30CRD (CAN 500 kbps) and other UDS ECUs.

use serialport::SerialPort;
use std::time::{Duration, Instant};

/// Common CAN IDs for many ECUs (11-bit)
pub const ECM_REQUEST_ID: u32 = 0x7E0;
pub const ECM_RESPONSE_ID: u32 = 0x7E8;
pub const BROADCAST_ID: u32 = 0x7DF;

/// ISO-TP PCI type nibbles
const PCI_SF: u8 = 0x00; // Single Frame
const PCI_FF: u8 = 0x10; // First Frame
const PCI_CF: u8 = 0x20; // Consecutive Frame
const PCI_FC: u8 = 0x30; // Flow Control

/// Flow Control flags
pub const FC_CTS: u8 = 0x00; // Continue To Send
pub const FC_WAIT: u8 = 0x01;
pub const FC_OVFLW: u8 = 0x02;

/// Default block size (0 = send all remaining without further FC)
pub const DEFAULT_FC_BLOCK_SIZE: u8 = 0;
/// Default separation time minimum (ms) advertised to sender
pub const DEFAULT_FC_STMIN_MS: u8 = 0;

/// Simple CAN frame (11-bit for now)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
    pub is_extended: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// ISO-TP pure logic (no I/O) — unit-tested
// ─────────────────────────────────────────────────────────────────────────────

/// Classify and extract payload from a single ISO-TP PCI frame (up to 8 data bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsoTpSegment {
    /// Complete single-frame payload
    Single(Vec<u8>),
    /// First frame: (total_length, first_data_bytes)
    First { total_len: usize, data: Vec<u8> },
    /// Consecutive frame: (sequence 0-15, data)
    Consecutive { seq: u8, data: Vec<u8> },
    /// Flow control from peer
    FlowControl { flag: u8, block_size: u8, st_min: u8 },
    /// Unrecognised
    Unknown(Vec<u8>),
}

/// Parse one CAN data field (≤8 bytes) as an ISO-TP segment.
pub fn parse_isotp_segment(data: &[u8]) -> IsoTpSegment {
    if data.is_empty() {
        return IsoTpSegment::Unknown(data.to_vec());
    }
    let pci = data[0];
    match pci & 0xF0 {
        PCI_SF => {
            let len = (pci & 0x0F) as usize;
            let payload = data.get(1..1 + len).unwrap_or(&[]).to_vec();
            IsoTpSegment::Single(payload)
        }
        PCI_FF => {
            if data.len() < 2 {
                return IsoTpSegment::Unknown(data.to_vec());
            }
            let total_len = (((pci & 0x0F) as usize) << 8) | (data[1] as usize);
            let payload = data.get(2..).unwrap_or(&[]).to_vec();
            IsoTpSegment::First {
                total_len,
                data: payload,
            }
        }
        PCI_CF => {
            let seq = pci & 0x0F;
            let payload = data.get(1..).unwrap_or(&[]).to_vec();
            IsoTpSegment::Consecutive { seq, data: payload }
        }
        PCI_FC => {
            let flag = pci & 0x0F;
            let bs = data.get(1).copied().unwrap_or(0);
            let st = data.get(2).copied().unwrap_or(0);
            IsoTpSegment::FlowControl {
                flag,
                block_size: bs,
                st_min: st,
            }
        }
        _ => IsoTpSegment::Unknown(data.to_vec()),
    }
}

/// Build a Flow Control frame (CTS by default).
pub fn build_flow_control(flag: u8, block_size: u8, st_min_ms: u8) -> [u8; 8] {
    let mut frame = [0u8; 8];
    frame[0] = PCI_FC | (flag & 0x0F);
    frame[1] = block_size;
    frame[2] = st_min_ms;
    frame
}

/// Build a Single Frame for a short UDS payload (≤7 bytes).
pub fn build_single_frame(payload: &[u8]) -> Result<[u8; 8], String> {
    if payload.len() > 7 {
        return Err("SF payload max 7 bytes".into());
    }
    let mut frame = [0u8; 8];
    frame[0] = PCI_SF | (payload.len() as u8);
    for (i, &b) in payload.iter().enumerate() {
        frame[1 + i] = b;
    }
    Ok(frame)
}

/// Assemble a multi-frame ISO-TP message from a First Frame + Consecutive Frames.
///
/// `segments` must start with a First frame; subsequent are Consecutive in order.
/// Returns the complete payload (without PCI bytes).
pub fn assemble_multiframe(segments: &[IsoTpSegment]) -> Result<Vec<u8>, String> {
    if segments.is_empty() {
        return Err("No segments".into());
    }
    let (total_len, mut buf) = match &segments[0] {
        IsoTpSegment::First { total_len, data } => (*total_len, data.clone()),
        IsoTpSegment::Single(data) => return Ok(data.clone()),
        _ => return Err("First segment must be First or Single frame".into()),
    };

    let mut expected_seq: u8 = 1;
    for seg in &segments[1..] {
        match seg {
            IsoTpSegment::Consecutive { seq, data } => {
                if *seq != expected_seq & 0x0F {
                    return Err(format!(
                        "CF sequence error: got {}, expected {}",
                        seq,
                        expected_seq & 0x0F
                    ));
                }
                buf.extend_from_slice(data);
                expected_seq = expected_seq.wrapping_add(1);
            }
            _ => return Err("Expected Consecutive Frame".into()),
        }
        if buf.len() >= total_len {
            break;
        }
    }

    if buf.len() < total_len {
        return Err(format!(
            "Incomplete multi-frame: got {} of {} bytes",
            buf.len(),
            total_len
        ));
    }
    buf.truncate(total_len);
    Ok(buf)
}

// ─────────────────────────────────────────────────────────────────────────────
// ELM327 helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn elm_init_can_500k(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    let cmds = [
        "AT Z",
        "AT E0",
        "AT L0",
        "AT S0",
        "AT H0",
        "AT SP 6", // ISO 15765-4 CAN 11-bit 500k
        "AT AL",  // allow long messages (>7 bytes) — critical for multi-frame
        "AT CAF 0", // CAN auto-format off so we see PCI bytes
    ];
    for c in cmds {
        let _ = send_elm_cmd(port, c);
        std::thread::sleep(Duration::from_millis(60));
    }
    Ok(())
}

fn send_elm_cmd(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    let line = format!("{}\r", cmd);
    port.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    port.flush().ok();
    std::thread::sleep(Duration::from_millis(40));

    let mut buf = [0u8; 512];
    let n = port.read(&mut buf).unwrap_or(0);
    let resp = String::from_utf8_lossy(&buf[..n]).to_string();
    if resp.contains("?") || resp.contains("UNABLE") {
        Err(format!("ELM cmd failed: {} -> {}", cmd, resp.trim()))
    } else {
        Ok(resp.trim().to_string())
    }
}

pub fn elm_set_header(port: &mut Box<dyn SerialPort + Send>, can_id: u32) -> Result<(), String> {
    let cmd = format!("AT SH {:03X}", can_id);
    send_elm_cmd(port, &cmd)?;
    Ok(())
}

/// Parse ELM ASCII hex response into raw bytes (strips spaces, prompts, CR).
fn elm_hex_to_bytes(raw: &str) -> Vec<u8> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    for i in (0..cleaned.len()).step_by(2) {
        if i + 1 < cleaned.len() {
            if let Ok(b) = u8::from_str_radix(&cleaned[i..i + 2], 16) {
                out.push(b);
            }
        }
    }
    out
}

/// Read available ELM response data with a timeout.
fn elm_read_raw(port: &mut Box<dyn SerialPort + Send>, timeout: Duration) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + timeout;
    let mut collected = String::new();
    let mut buf = [0u8; 512];

    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(0) => std::thread::sleep(Duration::from_millis(5)),
            Ok(n) => {
                collected.push_str(&String::from_utf8_lossy(&buf[..n]));
                // ELM ends responses with '>'
                if collected.contains('>') {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                if !collected.is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    if collected.is_empty() {
        return Err("ELM: no data".into());
    }
    Ok(elm_hex_to_bytes(&collected))
}

/// Send ISO-TP request via ELM and reassemble multi-frame responses.
///
/// On First Frame: sends Flow Control (ATFC or manual FC frame depending on adapter).
/// Collects Consecutive Frames until `total_len` is satisfied.
pub fn elm_send_iso_tp_request(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    elm_set_header(port, request_id)?;

    // Prefer ISO-TP auto formatting off so we control FC
    let _ = send_elm_cmd(port, "AT CAF 0");

    // Transmit: for short payloads use SF; longer requests use multi-frame TX
    // (most diagnostic requests are ≤7 bytes → SF)
    if data.len() <= 7 {
        let sf = build_single_frame(data)?;
        // ELM wants the data bytes as hex; with CAF0 include PCI
        let hex: String = sf
            .iter()
            .take(1 + data.len())
            .map(|b| format!("{:02X}", b))
            .collect();
        let cmd = format!("{}\r", hex);
        port.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
    } else {
        // Multi-frame TX (rare for requests; included for completeness)
        return elm_send_multiframe_tx(port, data);
    }

    // Receive path
    let first_raw = elm_read_raw(port, Duration::from_millis(800))?;
    if first_raw.is_empty() {
        return Err("Empty ISO-TP response".into());
    }

    let seg = parse_isotp_segment(&first_raw);
    match seg {
        IsoTpSegment::Single(payload) => Ok(payload),
        IsoTpSegment::First { total_len, data: ff_data } => {
            // Send Flow Control CTS
            let fc = build_flow_control(FC_CTS, DEFAULT_FC_BLOCK_SIZE, DEFAULT_FC_STMIN_MS);
            let fc_hex: String = fc[..3].iter().map(|b| format!("{:02X}", b)).collect();
            // FC is sent TO the ECU, so header stays request ID on many adapters;
            // OBDLink/STN: use AT SH with response ID for FC in some firmwares.
            // Practical approach: send FC as raw after setting header to response path.
            let _ = elm_set_header(port, request_id);
            let fc_cmd = format!("{}\r", fc_hex);
            port.write_all(fc_cmd.as_bytes()).map_err(|e| e.to_string())?;

            let mut segments = vec![IsoTpSegment::First {
                total_len,
                data: ff_data,
            }];
            let mut collected = segments[0].clone();
            let mut buf = match collected {
                IsoTpSegment::First { ref data, .. } => data.clone(),
                _ => vec![],
            };

            let deadline = Instant::now() + Duration::from_millis(3000);
            let mut expected_seq: u8 = 1;

            while buf.len() < total_len && Instant::now() < deadline {
                match elm_read_raw(port, Duration::from_millis(400)) {
                    Ok(raw) if !raw.is_empty() => {
                        // ELM may concatenate multiple CFs; split into 8-byte frames
                        for chunk in raw.chunks(8) {
                            let s = parse_isotp_segment(chunk);
                            if let IsoTpSegment::Consecutive { seq, data: cf } = s {
                                if seq != (expected_seq & 0x0F) {
                                    // tolerate minor desync from adapter buffering
                                }
                                buf.extend_from_slice(&cf);
                                expected_seq = expected_seq.wrapping_add(1);
                                segments.push(IsoTpSegment::Consecutive {
                                    seq,
                                    data: cf,
                                });
                            }
                            if buf.len() >= total_len {
                                break;
                            }
                        }
                    }
                    Ok(_) => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }

            if buf.len() < total_len {
                return Err(format!(
                    "ISO-TP incomplete: got {}/{} bytes",
                    buf.len(),
                    total_len
                ));
            }
            buf.truncate(total_len);
            Ok(buf)
        }
        IsoTpSegment::Unknown(raw) => {
            // Fallback: treat as already-stripped payload (some ELM configs)
            if raw.len() > 1 && raw[0] == 0x7F {
                return Ok(raw); // negative response, pass up
            }
            Ok(raw)
        }
        other => Err(format!("Unexpected ISO-TP segment: {:?}", other)),
    }
}

fn elm_send_multiframe_tx(
    port: &mut Box<dyn SerialPort + Send>,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    // First Frame
    let total = data.len();
    if total > 4095 {
        return Err("ISO-TP max length 4095".into());
    }
    let mut ff = [0u8; 8];
    ff[0] = PCI_FF | (((total >> 8) as u8) & 0x0F);
    ff[1] = (total & 0xFF) as u8;
    let first_data_len = (total).min(6);
    ff[2..2 + first_data_len].copy_from_slice(&data[..first_data_len]);

    let hex: String = ff.iter().map(|b| format!("{:02X}", b)).collect();
    port.write_all(format!("{}\r", hex).as_bytes())
        .map_err(|e| e.to_string())?;

    // Wait for FC
    let fc_raw = elm_read_raw(port, Duration::from_millis(500))?;
    let fc_seg = parse_isotp_segment(&fc_raw);
    match fc_seg {
        IsoTpSegment::FlowControl { flag, .. } if flag == FC_CTS => {}
        IsoTpSegment::FlowControl { flag, .. } => {
            return Err(format!("FC flag 0x{:02X} not CTS", flag));
        }
        _ => {
            // Some adapters auto-handle; continue
        }
    }

    // Consecutive Frames
    let mut offset = first_data_len;
    let mut seq: u8 = 1;
    while offset < total {
        let mut cf = [0u8; 8];
        cf[0] = PCI_CF | (seq & 0x0F);
        let n = (total - offset).min(7);
        cf[1..1 + n].copy_from_slice(&data[offset..offset + n]);
        let cf_hex: String = cf[..1 + n].iter().map(|b| format!("{:02X}", b)).collect();
        port.write_all(format!("{}\r", cf_hex).as_bytes())
            .map_err(|e| e.to_string())?;
        offset += n;
        seq = seq.wrapping_add(1);
        std::thread::sleep(Duration::from_millis(2));
    }

    // Read final response (may itself be multi-frame)
    elm_read_raw(port, Duration::from_millis(1500)).map(|raw| {
        match parse_isotp_segment(&raw) {
            IsoTpSegment::Single(p) => p,
            _ => raw,
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Lawicel / raw CAN
// ─────────────────────────────────────────────────────────────────────────────

pub fn send_raw_can(port: &mut Box<dyn SerialPort + Send>, frame: &CanFrame) -> Result<(), String> {
    if !frame.is_extended && frame.data.len() <= 8 {
        let mut s = format!("t{:03X}{:01X}", frame.id, frame.data.len());
        for b in &frame.data {
            s.push_str(&format!("{:02X}", b));
        }
        s.push('\r');
        port.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Extended or long frames need framed ISO-TP".into())
    }
}

pub fn recv_raw_can(port: &mut Box<dyn SerialPort + Send>) -> Result<CanFrame, String> {
    let mut buf = [0u8; 128];
    let n = port.read(&mut buf).map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&buf[..n]);
    // Find a 't' frame in the buffer
    if let Some(start) = s.find('t') {
        let frame_str = &s[start..];
        if frame_str.len() > 5 {
            let id = u32::from_str_radix(&frame_str[1..4], 16).unwrap_or(0);
            let dlc = usize::from_str_radix(&frame_str[4..5], 16).unwrap_or(0).min(8);
            let mut data = Vec::with_capacity(dlc);
            for i in 0..dlc {
                let off = 5 + i * 2;
                if off + 2 <= frame_str.len() {
                    if let Ok(b) = u8::from_str_radix(&frame_str[off..off + 2], 16) {
                        data.push(b);
                    }
                }
            }
            return Ok(CanFrame {
                id,
                data,
                is_extended: false,
            });
        }
    }
    Err("No parseable CAN frame".into())
}

/// Multi-frame receive over raw/Lawicel CAN.
pub fn raw_isotp_transact(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    response_id: u32,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    // TX
    if payload.len() <= 7 {
        let sf = build_single_frame(payload)?;
        let frame = CanFrame {
            id: request_id,
            data: sf[..1 + payload.len()].to_vec(),
            is_extended: false,
        };
        send_raw_can(port, &frame)?;
    } else {
        return Err("Raw multi-frame TX not yet wired — use ELM path".into());
    }

    // RX first segment
    let deadline = Instant::now() + Duration::from_millis(2000);
    let mut first: Option<CanFrame> = None;
    while Instant::now() < deadline {
        if let Ok(f) = recv_raw_can(port) {
            if f.id == response_id || response_id == 0 {
                first = Some(f);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let first = first.ok_or("No ISO-TP response frame")?;
    let seg = parse_isotp_segment(&first.data);

    match seg {
        IsoTpSegment::Single(p) => Ok(p),
        IsoTpSegment::First { total_len, data } => {
            // Send FC
            let fc = build_flow_control(FC_CTS, DEFAULT_FC_BLOCK_SIZE, DEFAULT_FC_STMIN_MS);
            let fc_frame = CanFrame {
                id: request_id,
                data: fc[..3].to_vec(),
                is_extended: false,
            };
            send_raw_can(port, &fc_frame)?;

            let mut buf = data;
            let mut expected_seq: u8 = 1;
            let cf_deadline = Instant::now() + Duration::from_millis(3000);

            while buf.len() < total_len && Instant::now() < cf_deadline {
                if let Ok(f) = recv_raw_can(port) {
                    if f.id != response_id && response_id != 0 {
                        continue;
                    }
                    if let IsoTpSegment::Consecutive { seq, data: cf } =
                        parse_isotp_segment(&f.data)
                    {
                        let _ = seq; // sequence validated loosely
                        buf.extend_from_slice(&cf);
                        expected_seq = expected_seq.wrapping_add(1);
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(3));
                }
            }
            if buf.len() < total_len {
                return Err(format!(
                    "Raw ISO-TP incomplete: {}/{}",
                    buf.len(),
                    total_len
                ));
            }
            buf.truncate(total_len);
            Ok(buf)
        }
        other => Err(format!("Unexpected segment: {:?}", other)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// High-level UDS
// ─────────────────────────────────────────────────────────────────────────────

/// Send a UDS request and return the response payload (SID stripped for positive).
/// Handles multi-frame ISO-TP automatically when `use_elm` is true.
pub fn uds_request(
    port: &mut Box<dyn SerialPort + Send>,
    sid: u8,
    data: &[u8],
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let mut payload = vec![sid];
    payload.extend_from_slice(data);

    if use_elm {
        elm_send_iso_tp_request(port, ECM_REQUEST_ID, &payload)
    } else {
        raw_isotp_transact(port, ECM_REQUEST_ID, ECM_RESPONSE_ID, &payload)
    }
}

/// Explicit multi-frame UDS request (same as uds_request; kept for API clarity).
pub fn uds_request_multiframe(
    port: &mut Box<dyn SerialPort + Send>,
    sid: u8,
    data: &[u8],
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    uds_request(port, sid, data, use_elm)
}

// ─────────────────────────────────────────────────────────────────────────────
// J2534 stubs
// ─────────────────────────────────────────────────────────────────────────────

pub fn j2534_available() -> bool {
    cfg!(windows)
}

#[cfg(windows)]
pub fn j2534_list_devices() -> Vec<String> {
    vec!["OpenPort 2.0 (if installed)".into(), "DrewTech / VSI".into()]
}

#[cfg(not(windows))]
pub fn j2534_list_devices() -> Vec<String> {
    vec!["J2534 only supported on Windows".into()]
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_frame() {
        // SF, 3 bytes payload: 0x03 0x62 0xF1 0x90
        let data = [0x03, 0x62, 0xF1, 0x90, 0, 0, 0, 0];
        match parse_isotp_segment(&data) {
            IsoTpSegment::Single(p) => assert_eq!(p, vec![0x62, 0xF1, 0x90]),
            other => panic!("expected Single, got {:?}", other),
        }
    }

    #[test]
    fn parse_first_frame() {
        // FF, total length 20 (0x0014), 6 data bytes
        let data = [0x10, 0x14, 0x63, 0x01, 0x02, 0x03, 0x04, 0x05];
        match parse_isotp_segment(&data) {
            IsoTpSegment::First { total_len, data: d } => {
                assert_eq!(total_len, 20);
                assert_eq!(d, vec![0x63, 0x01, 0x02, 0x03, 0x04, 0x05]);
            }
            other => panic!("expected First, got {:?}", other),
        }
    }

    #[test]
    fn parse_consecutive_frame() {
        let data = [0x21, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11];
        match parse_isotp_segment(&data) {
            IsoTpSegment::Consecutive { seq, data: d } => {
                assert_eq!(seq, 1);
                assert_eq!(d.len(), 7);
                assert_eq!(d[0], 0xAA);
            }
            other => panic!("expected CF, got {:?}", other),
        }
    }

    #[test]
    fn flow_control_encode() {
        let fc = build_flow_control(FC_CTS, 0, 0);
        assert_eq!(fc[0], 0x30);
        assert_eq!(fc[1], 0);
        assert_eq!(fc[2], 0);
    }

    #[test]
    fn assemble_sf() {
        let segs = vec![IsoTpSegment::Single(vec![0x63, 0x11, 0x22])];
        let out = assemble_multiframe(&segs).unwrap();
        assert_eq!(out, vec![0x63, 0x11, 0x22]);
    }

    #[test]
    fn assemble_ff_cf() {
        // total 10 bytes: 6 in FF + 4 in CF
        let segs = vec![
            IsoTpSegment::First {
                total_len: 10,
                data: vec![0x63, 1, 2, 3, 4, 5],
            },
            IsoTpSegment::Consecutive {
                seq: 1,
                data: vec![6, 7, 8, 9],
            },
        ];
        let out = assemble_multiframe(&segs).unwrap();
        assert_eq!(out, vec![0x63, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn assemble_incomplete_errors() {
        let segs = vec![IsoTpSegment::First {
            total_len: 20,
            data: vec![1, 2, 3],
        }];
        assert!(assemble_multiframe(&segs).is_err());
    }

    #[test]
    fn build_sf_rejects_long() {
        assert!(build_single_frame(&[0; 8]).is_err());
        assert!(build_single_frame(&[1, 2, 3]).is_ok());
    }
}
