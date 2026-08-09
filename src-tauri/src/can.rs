#![allow(unused, dead_code)]
//! can.rs — CAN / ISO-TP (ISO 15765-2) support for TuneItVerse
//!
//! - ELM327 / STN / OBDLink over serial
//! - Lawicel-style raw CAN
//! - Full ISO-TP: SF / FF / CF + Flow Control (CTS)
//! - Configurable BS / STmin with automatic gentler retry on incomplete RX

use serialport::SerialPort;
use std::time::{Duration, Instant};

pub const ECM_REQUEST_ID: u32 = 0x7E0;
pub const ECM_RESPONSE_ID: u32 = 0x7E8;
pub const BROADCAST_ID: u32 = 0x7DF;

const PCI_SF: u8 = 0x00;
const PCI_FF: u8 = 0x10;
const PCI_CF: u8 = 0x20;
const PCI_FC: u8 = 0x30;

pub const FC_CTS: u8 = 0x00;
pub const FC_WAIT: u8 = 0x01;
pub const FC_OVFLW: u8 = 0x02;

pub const DEFAULT_FC_BLOCK_SIZE: u8 = 0;
pub const DEFAULT_FC_STMIN_MS: u8 = 0;

/// Gentler FC used on incomplete multi-frame RX retry (cheap ELM clones).
pub const RETRY_FC_BLOCK_SIZE: u8 = 16;
pub const RETRY_FC_STMIN_MS: u8 = 2;

/// Flow Control parameters advertised to the sender (ECU) after a First Frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowControlParams {
    pub block_size: u8,
    pub st_min_ms: u8,
}

impl Default for FlowControlParams {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_FC_BLOCK_SIZE,
            st_min_ms: DEFAULT_FC_STMIN_MS,
        }
    }
}

impl FlowControlParams {
    pub fn fast() -> Self {
        Self::default()
    }
    /// Safer pacing for weak adapters / large Mode 23 windows.
    pub fn gentle() -> Self {
        Self {
            block_size: RETRY_FC_BLOCK_SIZE,
            st_min_ms: RETRY_FC_STMIN_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
    pub is_extended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsoTpSegment {
    Single(Vec<u8>),
    First { total_len: usize, data: Vec<u8> },
    Consecutive { seq: u8, data: Vec<u8> },
    FlowControl { flag: u8, block_size: u8, st_min: u8 },
    Unknown(Vec<u8>),
}

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

pub fn build_flow_control(flag: u8, block_size: u8, st_min_ms: u8) -> [u8; 8] {
    let mut frame = [0u8; 8];
    frame[0] = PCI_FC | (flag & 0x0F);
    frame[1] = block_size;
    frame[2] = st_min_ms;
    frame
}

pub fn build_flow_control_params(fc: FlowControlParams) -> [u8; 8] {
    build_flow_control(FC_CTS, fc.block_size, fc.st_min_ms)
}

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

// ── ELM327 ──────────────────────────────────────────────────────────────────

pub fn elm_init_can_500k(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    let cmds = [
        "AT Z",
        "AT E0",
        "AT L0",
        "AT S0",
        "AT H0",
        "AT SP 6",
        "AT AL",
        "AT CAF 0",
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

fn elm_hex_to_bytes(raw: &str) -> Vec<u8> {
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
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

fn elm_read_raw(port: &mut Box<dyn SerialPort + Send>, timeout: Duration) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + timeout;
    let mut collected = String::new();
    let mut buf = [0u8; 512];

    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(0) => std::thread::sleep(Duration::from_millis(5)),
            Ok(n) => {
                collected.push_str(&String::from_utf8_lossy(&buf[..n]));
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

fn elm_collect_cfs(
    port: &mut Box<dyn SerialPort + Send>,
    total_len: usize,
    mut buf: Vec<u8>,
    overall_ms: u64,
) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + Duration::from_millis(overall_ms);
    let mut expected_seq: u8 = 1;

    while buf.len() < total_len && Instant::now() < deadline {
        match elm_read_raw(port, Duration::from_millis(400)) {
            Ok(raw) if !raw.is_empty() => {
                for chunk in raw.chunks(8) {
                    if let IsoTpSegment::Consecutive { seq, data: cf } = parse_isotp_segment(chunk) {
                        let _ = seq;
                        let _ = expected_seq;
                        buf.extend_from_slice(&cf);
                        expected_seq = expected_seq.wrapping_add(1);
                    }
                    if buf.len() >= total_len {
                        break;
                    }
                }
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
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

/// ISO-TP request with explicit Flow Control parameters.
pub fn elm_send_iso_tp_request_with_fc(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
    fc: FlowControlParams,
) -> Result<Vec<u8>, String> {
    elm_set_header(port, request_id)?;
    let _ = send_elm_cmd(port, "AT CAF 0");

    if data.len() <= 7 {
        let sf = build_single_frame(data)?;
        let hex: String = sf
            .iter()
            .take(1 + data.len())
            .map(|b| format!("{:02X}", b))
            .collect();
        port.write_all(format!("{}\r", hex).as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        return elm_send_multiframe_tx(port, data);
    }

    let first_raw = elm_read_raw(port, Duration::from_millis(800))?;
    if first_raw.is_empty() {
        return Err("Empty ISO-TP response".into());
    }

    match parse_isotp_segment(&first_raw) {
        IsoTpSegment::Single(payload) => Ok(payload),
        IsoTpSegment::First {
            total_len,
            data: ff_data,
        } => {
            let fc_frame = build_flow_control_params(fc);
            let fc_hex: String = fc_frame[..3].iter().map(|b| format!("{:02X}", b)).collect();
            let _ = elm_set_header(port, request_id);
            port.write_all(format!("{}\r", fc_hex).as_bytes())
                .map_err(|e| e.to_string())?;

            elm_collect_cfs(port, total_len, ff_data, 3000)
        }
        IsoTpSegment::Unknown(raw) => {
            if raw.len() > 1 && raw[0] == 0x7F {
                return Ok(raw);
            }
            Ok(raw)
        }
        other => Err(format!("Unexpected ISO-TP segment: {:?}", other)),
    }
}

/// Default path: fast FC first; on incomplete multi-frame, one gentler retry.
pub fn elm_send_iso_tp_request(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    match elm_send_iso_tp_request_with_fc(port, request_id, data, FlowControlParams::fast()) {
        Ok(payload) => Ok(payload),
        Err(e) if e.contains("incomplete") => {
            // Adapter may have dropped CFs under BS=0/STmin=0 — retry paced
            let _ = send_elm_cmd(port, "AT CAF 0");
            std::thread::sleep(Duration::from_millis(30));
            elm_send_iso_tp_request_with_fc(port, request_id, data, FlowControlParams::gentle())
                .map_err(|e2| format!("{} (gentle retry: {})", e, e2))
        }
        Err(e) => Err(e),
    }
}

fn elm_send_multiframe_tx(
    port: &mut Box<dyn SerialPort + Send>,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let total = data.len();
    if total > 4095 {
        return Err("ISO-TP max length 4095".into());
    }
    let mut ff = [0u8; 8];
    ff[0] = PCI_FF | (((total >> 8) as u8) & 0x0F);
    ff[1] = (total & 0xFF) as u8;
    let first_data_len = total.min(6);
    ff[2..2 + first_data_len].copy_from_slice(&data[..first_data_len]);

    let hex: String = ff.iter().map(|b| format!("{:02X}", b)).collect();
    port.write_all(format!("{}\r", hex).as_bytes())
        .map_err(|e| e.to_string())?;

    let fc_raw = elm_read_raw(port, Duration::from_millis(500))?;
    match parse_isotp_segment(&fc_raw) {
        IsoTpSegment::FlowControl { flag, .. } if flag == FC_CTS => {}
        IsoTpSegment::FlowControl { flag, .. } => {
            return Err(format!("FC flag 0x{:02X} not CTS", flag));
        }
        _ => {}
    }

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

    elm_read_raw(port, Duration::from_millis(1500)).map(|raw| match parse_isotp_segment(&raw) {
        IsoTpSegment::Single(p) => p,
        _ => raw,
    })
}

// ── Lawicel / raw CAN ───────────────────────────────────────────────────────

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

pub fn raw_isotp_transact_with_fc(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    response_id: u32,
    payload: &[u8],
    fc: FlowControlParams,
) -> Result<Vec<u8>, String> {
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

    match parse_isotp_segment(&first.data) {
        IsoTpSegment::Single(p) => Ok(p),
        IsoTpSegment::First { total_len, data } => {
            let fc_bytes = build_flow_control_params(fc);
            let fc_frame = CanFrame {
                id: request_id,
                data: fc_bytes[..3].to_vec(),
                is_extended: false,
            };
            send_raw_can(port, &fc_frame)?;

            let mut buf = data;
            let cf_deadline = Instant::now() + Duration::from_millis(3000);
            while buf.len() < total_len && Instant::now() < cf_deadline {
                if let Ok(f) = recv_raw_can(port) {
                    if f.id != response_id && response_id != 0 {
                        continue;
                    }
                    if let IsoTpSegment::Consecutive { data: cf, .. } =
                        parse_isotp_segment(&f.data)
                    {
                        buf.extend_from_slice(&cf);
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(3));
                }
            }
            if buf.len() < total_len {
                return Err(format!("Raw ISO-TP incomplete: {}/{}", buf.len(), total_len));
            }
            buf.truncate(total_len);
            Ok(buf)
        }
        other => Err(format!("Unexpected segment: {:?}", other)),
    }
}

pub fn raw_isotp_transact(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    response_id: u32,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    match raw_isotp_transact_with_fc(
        port,
        request_id,
        response_id,
        payload,
        FlowControlParams::fast(),
    ) {
        Ok(p) => Ok(p),
        Err(e) if e.contains("incomplete") => raw_isotp_transact_with_fc(
            port,
            request_id,
            response_id,
            payload,
            FlowControlParams::gentle(),
        )
        .map_err(|e2| format!("{} (gentle retry: {})", e, e2)),
        Err(e) => Err(e),
    }
}

// ── High-level UDS ──────────────────────────────────────────────────────────

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

pub fn uds_request_multiframe(
    port: &mut Box<dyn SerialPort + Send>,
    sid: u8,
    data: &[u8],
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    uds_request(port, sid, data, use_elm)
}

/// Multi-frame UDS with explicit FC (no automatic retry).
pub fn uds_request_with_fc(
    port: &mut Box<dyn SerialPort + Send>,
    sid: u8,
    data: &[u8],
    use_elm: bool,
    fc: FlowControlParams,
) -> Result<Vec<u8>, String> {
    let mut payload = vec![sid];
    payload.extend_from_slice(data);
    if use_elm {
        elm_send_iso_tp_request_with_fc(port, ECM_REQUEST_ID, &payload, fc)
    } else {
        raw_isotp_transact_with_fc(port, ECM_REQUEST_ID, ECM_RESPONSE_ID, &payload, fc)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_frame() {
        let data = [0x03, 0x62, 0xF1, 0x90, 0, 0, 0, 0];
        match parse_isotp_segment(&data) {
            IsoTpSegment::Single(p) => assert_eq!(p, vec![0x62, 0xF1, 0x90]),
            other => panic!("expected Single, got {:?}", other),
        }
    }

    #[test]
    fn parse_first_frame() {
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
    fn flow_control_encode() {
        let fc = build_flow_control(FC_CTS, 0, 0);
        assert_eq!(fc[0], 0x30);
        assert_eq!(fc[1], 0);
        assert_eq!(fc[2], 0);

        let gentle = build_flow_control_params(FlowControlParams::gentle());
        assert_eq!(gentle[0], 0x30);
        assert_eq!(gentle[1], RETRY_FC_BLOCK_SIZE);
        assert_eq!(gentle[2], RETRY_FC_STMIN_MS);
    }

    #[test]
    fn assemble_ff_cf() {
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
    fn fc_params_defaults() {
        assert_eq!(FlowControlParams::fast().block_size, 0);
        assert_eq!(FlowControlParams::gentle().block_size, 16);
    }
}
