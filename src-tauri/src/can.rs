#![allow(unused, dead_code)]
//! can.rs — CAN / ISO-TP (ISO 15765-2) for TuneItVerse
//!
//! Architecture:
//!   1. Pure ISO-TP layer  — parse/encode SF·FF·CF·FC, MultiframeAssembler
//!   2. Transport adapters — ELM327 ASCII, Lawicel raw CAN
//!   3. UDS façade         — sid + payload → multi-frame aware request
//!
//! Flow control is owned by the pure layer (`FlowControlParams`, `FcFlag`,
//! `FlowControlFrame`). Transports only send/receive the encoded bytes.

use serialport::SerialPort;
use std::time::{Duration, Instant};

// ── Addressing ──────────────────────────────────────────────────────────────

pub const ECM_REQUEST_ID: u32 = 0x7E0;
pub const ECM_RESPONSE_ID: u32 = 0x7E8;
pub const BROADCAST_ID: u32 = 0x7DF;

// ── PCI type nibbles ────────────────────────────────────────────────────────

const PCI_SF: u8 = 0x00;
const PCI_FF: u8 = 0x10;
const PCI_CF: u8 = 0x20;
const PCI_FC: u8 = 0x30;

// ── Flow Control ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FcFlag {
    Cts = 0x00,
    Wait = 0x01,
    Overflow = 0x02,
}

impl FcFlag {
    pub fn from_nibble(n: u8) -> Option<Self> {
        match n & 0x0F {
            0x00 => Some(Self::Cts),
            0x01 => Some(Self::Wait),
            0x02 => Some(Self::Overflow),
            _ => None,
        }
    }
    pub fn as_u8(self) -> u8 { self as u8 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowControlParams {
    pub block_size: u8,
    pub st_min_ms: u8,
}

impl Default for FlowControlParams {
    fn default() -> Self { Self::fast() }
}

impl FlowControlParams {
    pub const fn fast() -> Self { Self { block_size: 0, st_min_ms: 0 } }
    pub const fn gentle() -> Self { Self { block_size: 16, st_min_ms: 2 } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowControlFrame {
    pub flag: FcFlag,
    pub block_size: u8,
    pub st_min: u8,
}

impl FlowControlFrame {
    pub fn cts(params: FlowControlParams) -> Self {
        Self { flag: FcFlag::Cts, block_size: params.block_size, st_min: params.st_min_ms }
    }
    pub fn encode(self) -> [u8; 8] {
        let mut frame = [0u8; 8];
        frame[0] = PCI_FC | self.flag.as_u8();
        frame[1] = self.block_size;
        frame[2] = self.st_min;
        frame
    }
    pub fn encode_short(self) -> [u8; 3] {
        [PCI_FC | self.flag.as_u8(), self.block_size, self.st_min]
    }
    pub fn try_parse(data: &[u8]) -> Option<Self> {
        if data.is_empty() || (data[0] & 0xF0) != PCI_FC { return None; }
        let flag = FcFlag::from_nibble(data[0])?;
        Some(Self {
            flag,
            block_size: data.get(1).copied().unwrap_or(0),
            st_min: data.get(2).copied().unwrap_or(0),
        })
    }
    pub fn interpret_for_sender(self) -> Result<FlowControlParams, String> {
        match self.flag {
            FcFlag::Cts => Ok(FlowControlParams { block_size: self.block_size, st_min_ms: self.st_min }),
            FcFlag::Wait => Err("FC WAIT — peer not ready".into()),
            FcFlag::Overflow => Err("FC OVFLW — peer buffer overflow, abort".into()),
        }
    }
}

pub const FC_CTS: u8 = 0x00;
pub const FC_WAIT: u8 = 0x01;
pub const FC_OVFLW: u8 = 0x02;
pub const DEFAULT_FC_BLOCK_SIZE: u8 = 0;
pub const DEFAULT_FC_STMIN_MS: u8 = 0;
pub const RETRY_FC_BLOCK_SIZE: u8 = 16;
pub const RETRY_FC_STMIN_MS: u8 = 2;

pub fn build_flow_control(flag: u8, block_size: u8, st_min_ms: u8) -> [u8; 8] {
    let mut frame = [0u8; 8];
    frame[0] = PCI_FC | (flag & 0x0F);
    frame[1] = block_size;
    frame[2] = st_min_ms;
    frame
}

pub fn build_flow_control_params(fc: FlowControlParams) -> [u8; 8] {
    FlowControlFrame::cts(fc).encode()
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
    FlowControl(FlowControlFrame),
    Unknown(Vec<u8>),
}

pub fn parse_isotp_segment(data: &[u8]) -> IsoTpSegment {
    if data.is_empty() { return IsoTpSegment::Unknown(data.to_vec()); }
    match data[0] & 0xF0 {
        PCI_SF => {
            let len = (data[0] & 0x0F) as usize;
            IsoTpSegment::Single(data.get(1..1 + len).unwrap_or(&[]).to_vec())
        }
        PCI_FF => {
            if data.len() < 2 { return IsoTpSegment::Unknown(data.to_vec()); }
            let total_len = (((data[0] & 0x0F) as usize) << 8) | (data[1] as usize);
            IsoTpSegment::First { total_len, data: data.get(2..).unwrap_or(&[]).to_vec() }
        }
        PCI_CF => IsoTpSegment::Consecutive {
            seq: data[0] & 0x0F,
            data: data.get(1..).unwrap_or(&[]).to_vec(),
        },
        PCI_FC => match FlowControlFrame::try_parse(data) {
            Some(fc) => IsoTpSegment::FlowControl(fc),
            None => IsoTpSegment::Unknown(data.to_vec()),
        },
        _ => IsoTpSegment::Unknown(data.to_vec()),
    }
}

pub fn build_single_frame(payload: &[u8]) -> Result<[u8; 8], String> {
    if payload.len() > 7 { return Err("SF payload max 7 bytes".into()); }
    let mut frame = [0u8; 8];
    frame[0] = PCI_SF | (payload.len() as u8);
    frame[1..1 + payload.len()].copy_from_slice(payload);
    Ok(frame)
}

#[derive(Debug, Clone)]
pub struct MultiframeAssembler {
    total_len: usize,
    buf: Vec<u8>,
    expected_seq: u8,
    strict_seq: bool,
}

impl MultiframeAssembler {
    pub fn from_first(total_len: usize, first_data: Vec<u8>) -> Self {
        Self { total_len, buf: first_data, expected_seq: 1, strict_seq: false }
    }
    pub fn with_strict_seq(mut self, strict: bool) -> Self { self.strict_seq = strict; self }
    pub fn is_complete(&self) -> bool { self.buf.len() >= self.total_len }
    pub fn total_len(&self) -> usize { self.total_len }
    pub fn received(&self) -> usize { self.buf.len().min(self.total_len) }
    pub fn push_cf(&mut self, seq: u8, data: &[u8]) -> Result<bool, String> {
        if self.strict_seq && seq != (self.expected_seq & 0x0F) {
            return Err(format!("CF sequence error: got {}, expected {}", seq, self.expected_seq & 0x0F));
        }
        self.buf.extend_from_slice(data);
        self.expected_seq = self.expected_seq.wrapping_add(1);
        Ok(self.is_complete())
    }
    pub fn push_segment(&mut self, seg: &IsoTpSegment) -> Result<bool, String> {
        match seg {
            IsoTpSegment::Consecutive { seq, data } => self.push_cf(*seq, data),
            _ => Ok(self.is_complete()),
        }
    }
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        if self.buf.len() < self.total_len {
            return Err(format!("ISO-TP incomplete: got {}/{} bytes", self.buf.len(), self.total_len));
        }
        self.buf.truncate(self.total_len);
        Ok(self.buf)
    }
}

pub fn assemble_multiframe(segments: &[IsoTpSegment]) -> Result<Vec<u8>, String> {
    if segments.is_empty() { return Err("No segments".into()); }
    match &segments[0] {
        IsoTpSegment::Single(data) => Ok(data.clone()),
        IsoTpSegment::First { total_len, data } => {
            let mut asm = MultiframeAssembler::from_first(*total_len, data.clone()).with_strict_seq(true);
            for seg in &segments[1..] {
                if asm.push_segment(seg)? { break; }
            }
            asm.finish()
        }
        _ => Err("First segment must be First or Single frame".into()),
    }
}

fn with_fc_retry<F>(mut op: F) -> Result<Vec<u8>, String>
where F: FnMut(FlowControlParams) -> Result<Vec<u8>, String>,
{
    match op(FlowControlParams::fast()) {
        Ok(v) => Ok(v),
        Err(e) if e.contains("incomplete") => op(FlowControlParams::gentle())
            .map_err(|e2| format!("{} (gentle retry: {})", e, e2)),
        Err(e) => Err(e),
    }
}

// ── ELM327 transport ────────────────────────────────────────────────────────

pub fn elm_init_can_500k(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    for c in ["AT Z", "AT E0", "AT L0", "AT S0", "AT H0", "AT SP 6", "AT AL", "AT CAF 0"] {
        let _ = send_elm_cmd(port, c);
        std::thread::sleep(Duration::from_millis(60));
    }
    Ok(())
}

fn send_elm_cmd(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    port.write_all(format!("{}\r", cmd).as_bytes()).map_err(|e| e.to_string())?;
    port.flush().ok();
    std::thread::sleep(Duration::from_millis(40));
    let mut buf = [0u8; 512];
    let n = port.read(&mut buf).unwrap_or(0);
    let resp = String::from_utf8_lossy(&buf[..n]).to_string();
    if resp.contains('?') || resp.contains("UNABLE") {
        Err(format!("ELM cmd failed: {} -> {}", cmd, resp.trim()))
    } else {
        Ok(resp.trim().to_string())
    }
}

pub fn elm_set_header(port: &mut Box<dyn SerialPort + Send>, can_id: u32) -> Result<(), String> {
    send_elm_cmd(port, &format!("AT SH {:03X}", can_id))?;
    Ok(())
}

fn elm_hex_to_bytes(raw: &str) -> Vec<u8> {
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    for i in (0..cleaned.len()).step_by(2) {
        if i + 1 < cleaned.len() {
            if let Ok(b) = u8::from_str_radix(&cleaned[i..i + 2], 16) { out.push(b); }
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
                if collected.contains('>') { break; }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                if !collected.is_empty() { break; }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    if collected.is_empty() { return Err("ELM: no data".into()); }
    Ok(elm_hex_to_bytes(&collected))
}

fn elm_send_fc(port: &mut Box<dyn SerialPort + Send>, request_id: u32, fc: FlowControlParams) -> Result<(), String> {
    let bytes = FlowControlFrame::cts(fc).encode_short();
    let hex: String = bytes.iter().map(|b| format!("{:02X}", b)).collect();
    let _ = elm_set_header(port, request_id);
    port.write_all(format!("{}\r", hex).as_bytes()).map_err(|e| e.to_string())
}

fn elm_collect_cfs(
    port: &mut Box<dyn SerialPort + Send>,
    mut asm: MultiframeAssembler,
    overall_ms: u64,
) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + Duration::from_millis(overall_ms);
    while !asm.is_complete() && Instant::now() < deadline {
        match elm_read_raw(port, Duration::from_millis(400)) {
            Ok(raw) if !raw.is_empty() => {
                for chunk in raw.chunks(8) {
                    let _ = asm.push_segment(&parse_isotp_segment(chunk));
                    if asm.is_complete() { break; }
                }
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    asm.finish()
}

pub fn elm_send_iso_tp_request_with_fc(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
    fc: FlowControlParams,
) -> Result<Vec<u8>, String> {
    elm_set_header(port, request_id)?;
    let _ = send_elm_cmd(port, "AT CAF 0");
    if data.len() > 7 { return elm_send_multiframe_tx(port, data); }
    let sf = build_single_frame(data)?;
    let hex: String = sf.iter().take(1 + data.len()).map(|b| format!("{:02X}", b)).collect();
    port.write_all(format!("{}\r", hex).as_bytes()).map_err(|e| e.to_string())?;
    let first_raw = elm_read_raw(port, Duration::from_millis(800))?;
    if first_raw.is_empty() { return Err("Empty ISO-TP response".into()); }
    match parse_isotp_segment(&first_raw) {
        IsoTpSegment::Single(payload) => Ok(payload),
        IsoTpSegment::First { total_len, data: ff } => {
            elm_send_fc(port, request_id, fc)?;
            elm_collect_cfs(port, MultiframeAssembler::from_first(total_len, ff), 3000)
        }
        IsoTpSegment::Unknown(raw) if raw.first() == Some(&0x7F) => Ok(raw),
        IsoTpSegment::Unknown(raw) => Ok(raw),
        other => Err(format!("Unexpected ISO-TP segment: {:?}", other)),
    }
}

pub fn elm_send_iso_tp_request(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    with_fc_retry(|fc| {
        if fc == FlowControlParams::gentle() {
            let _ = send_elm_cmd(port, "AT CAF 0");
            std::thread::sleep(Duration::from_millis(30));
        }
        elm_send_iso_tp_request_with_fc(port, request_id, data, fc)
    })
}

fn elm_send_multiframe_tx(
    port: &mut Box<dyn SerialPort + Send>,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let total = data.len();
    if total > 4095 { return Err("ISO-TP max length 4095".into()); }
    let mut ff = [0u8; 8];
    ff[0] = PCI_FF | (((total >> 8) as u8) & 0x0F);
    ff[1] = (total & 0xFF) as u8;
    let first_data_len = total.min(6);
    ff[2..2 + first_data_len].copy_from_slice(&data[..first_data_len]);
    port.write_all(format!("{}\r", ff.iter().map(|b| format!("{:02X}", b)).collect::<String>()).as_bytes())
        .map_err(|e| e.to_string())?;
    let mut peer_params = FlowControlParams::fast();
    for attempt in 0..4 {
        let fc_raw = elm_read_raw(port, Duration::from_millis(500))?;
        match parse_isotp_segment(&fc_raw) {
            IsoTpSegment::FlowControl(frame) => match frame.interpret_for_sender() {
                Ok(p) => { peer_params = p; break; }
                Err(e) if e.contains("WAIT") && attempt < 3 => {
                    std::thread::sleep(Duration::from_millis(50)); continue;
                }
                Err(e) => return Err(e),
            },
            _ => break,
        }
    }
    let gap = Duration::from_millis(peer_params.st_min_ms.max(2) as u64);
    let mut offset = first_data_len;
    let mut seq: u8 = 1;
    let mut since_fc: u8 = 0;
    while offset < total {
        if peer_params.block_size != 0 && since_fc >= peer_params.block_size {
            return Err("TX block size exhausted without further FC".into());
        }
        let mut cf = [0u8; 8];
        cf[0] = PCI_CF | (seq & 0x0F);
        let n = (total - offset).min(7);
        cf[1..1 + n].copy_from_slice(&data[offset..offset + n]);
        port.write_all(
            format!("{}\r", cf[..1 + n].iter().map(|b| format!("{:02X}", b)).collect::<String>()).as_bytes(),
        ).map_err(|e| e.to_string())?;
        offset += n;
        seq = seq.wrapping_add(1);
        since_fc = since_fc.wrapping_add(1);
        std::thread::sleep(gap);
    }
    elm_read_raw(port, Duration::from_millis(1500)).map(|raw| match parse_isotp_segment(&raw) {
        IsoTpSegment::Single(p) => p,
        _ => raw,
    })
}

// ── Lawicel / raw CAN ───────────────────────────────────────────────────────

pub fn send_raw_can(port: &mut Box<dyn SerialPort + Send>, frame: &CanFrame) -> Result<(), String> {
    if frame.is_extended || frame.data.len() > 8 {
        return Err("Extended or long frames need framed ISO-TP".into());
    }
    let mut s = format!("t{:03X}{:01X}", frame.id, frame.data.len());
    for b in &frame.data { s.push_str(&format!("{:02X}", b)); }
    s.push('\r');
    port.write_all(s.as_bytes()).map_err(|e| e.to_string())
}

pub fn recv_raw_can(port: &mut Box<dyn SerialPort + Send>) -> Result<CanFrame, String> {
    let mut buf = [0u8; 128];
    let n = port.read(&mut buf).map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&buf[..n]);
    let start = s.find('t').ok_or("No parseable CAN frame")?;
    let frame_str = &s[start..];
    if frame_str.len() <= 5 { return Err("No parseable CAN frame".into()); }
    let id = u32::from_str_radix(&frame_str[1..4], 16).unwrap_or(0);
    let dlc = usize::from_str_radix(&frame_str[4..5], 16).unwrap_or(0).min(8);
    let mut data = Vec::with_capacity(dlc);
    for i in 0..dlc {
        let off = 5 + i * 2;
        if off + 2 <= frame_str.len() {
            if let Ok(b) = u8::from_str_radix(&frame_str[off..off + 2], 16) { data.push(b); }
        }
    }
    Ok(CanFrame { id, data, is_extended: false })
}

fn raw_send_multiframe_tx(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    response_id: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let total = data.len();
    if total > 4095 { return Err("ISO-TP max length 4095".into()); }
    let mut ff = [0u8; 8];
    ff[0] = PCI_FF | (((total >> 8) as u8) & 0x0F);
    ff[1] = (total & 0xFF) as u8;
    let first_data_len = total.min(6);
    ff[2..2 + first_data_len].copy_from_slice(&data[..first_data_len]);
    send_raw_can(port, &CanFrame { id: request_id, data: ff.to_vec(), is_extended: false })?;

    let mut peer_params = FlowControlParams::fast();
    let fc_deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < fc_deadline {
        if let Ok(f) = recv_raw_can(port) {
            if f.id != response_id && response_id != 0 { continue; }
            if let IsoTpSegment::FlowControl(frame) = parse_isotp_segment(&f.data) {
                match frame.interpret_for_sender() {
                    Ok(p) => { peer_params = p; break; }
                    Err(e) if e.contains("WAIT") => {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    let gap = Duration::from_millis(peer_params.st_min_ms.max(1) as u64);
    let mut offset = first_data_len;
    let mut seq: u8 = 1;
    let mut since_fc: u8 = 0;
    while offset < total {
        if peer_params.block_size != 0 && since_fc >= peer_params.block_size {
            // Wait for next FC grant
            let wait_dl = Instant::now() + Duration::from_millis(500);
            let mut got = false;
            while Instant::now() < wait_dl {
                if let Ok(f) = recv_raw_can(port) {
                    if let IsoTpSegment::FlowControl(frame) = parse_isotp_segment(&f.data) {
                        if let Ok(p) = frame.interpret_for_sender() {
                            peer_params = p;
                            since_fc = 0;
                            got = true;
                            break;
                        }
                    }
                }
            }
            if !got { return Err("TX block size exhausted without further FC".into()); }
        }
        let mut cf = [0u8; 8];
        cf[0] = PCI_CF | (seq & 0x0F);
        let n = (total - offset).min(7);
        cf[1..1 + n].copy_from_slice(&data[offset..offset + n]);
        send_raw_can(port, &CanFrame { id: request_id, data: cf[..1 + n].to_vec(), is_extended: false })?;
        offset += n;
        seq = seq.wrapping_add(1);
        since_fc = since_fc.wrapping_add(1);
        std::thread::sleep(gap);
    }

    // Read final response (may be multi-frame)
    let deadline = Instant::now() + Duration::from_millis(2000);
    while Instant::now() < deadline {
        if let Ok(f) = recv_raw_can(port) {
            if f.id != response_id && response_id != 0 { continue; }
            match parse_isotp_segment(&f.data) {
                IsoTpSegment::Single(p) => return Ok(p),
                IsoTpSegment::First { total_len, data: ff } => {
                    let fc = FlowControlFrame::cts(FlowControlParams::fast()).encode_short();
                    send_raw_can(port, &CanFrame { id: request_id, data: fc.to_vec(), is_extended: false })?;
                    let mut asm = MultiframeAssembler::from_first(total_len, ff);
                    let cf_dl = Instant::now() + Duration::from_millis(3000);
                    while !asm.is_complete() && Instant::now() < cf_dl {
                        if let Ok(cf) = recv_raw_can(port) {
                            if cf.id != response_id && response_id != 0 { continue; }
                            let _ = asm.push_segment(&parse_isotp_segment(&cf.data));
                        }
                    }
                    return asm.finish();
                }
                other => return Ok(f.data),
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err("Raw multi-frame TX: no final response".into())
}

pub fn raw_isotp_transact_with_fc(
    port: &mut Box<dyn SerialPort + Send>,
    request_id: u32,
    response_id: u32,
    payload: &[u8],
    fc: FlowControlParams,
) -> Result<Vec<u8>, String> {
    if payload.len() > 7 {
        return raw_send_multiframe_tx(port, request_id, response_id, payload);
    }
    let sf = build_single_frame(payload)?;
    send_raw_can(port, &CanFrame {
        id: request_id,
        data: sf[..1 + payload.len()].to_vec(),
        is_extended: false,
    })?;

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
            let fc_bytes = FlowControlFrame::cts(fc).encode_short();
            send_raw_can(port, &CanFrame {
                id: request_id,
                data: fc_bytes.to_vec(),
                is_extended: false,
            })?;
            let mut asm = MultiframeAssembler::from_first(total_len, data);
            let cf_deadline = Instant::now() + Duration::from_millis(3000);
            while !asm.is_complete() && Instant::now() < cf_deadline {
                if let Ok(f) = recv_raw_can(port) {
                    if f.id != response_id && response_id != 0 { continue; }
                    let _ = asm.push_segment(&parse_isotp_segment(&f.data));
                } else {
                    std::thread::sleep(Duration::from_millis(3));
                }
            }
            asm.finish()
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
    with_fc_retry(|fc| raw_isotp_transact_with_fc(port, request_id, response_id, payload, fc))
}

// ── UDS façade ──────────────────────────────────────────────────────────────

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

pub fn j2534_available() -> bool { cfg!(windows) }

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
    fn fc_flag_roundtrip() {
        assert_eq!(FcFlag::from_nibble(0x00), Some(FcFlag::Cts));
        assert_eq!(FcFlag::from_nibble(0x01), Some(FcFlag::Wait));
        assert_eq!(FcFlag::from_nibble(0x02), Some(FcFlag::Overflow));
    }

    #[test]
    fn assembler_completes() {
        let mut a = MultiframeAssembler::from_first(10, vec![0x63, 1, 2, 3, 4, 5]);
        a.push_cf(1, &[6, 7, 8, 9]).unwrap();
        assert!(a.is_complete());
        assert_eq!(a.finish().unwrap(), vec![0x63, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn parse_segments() {
        match parse_isotp_segment(&[0x03, 0x62, 0xF1, 0x90]) {
            IsoTpSegment::Single(p) => assert_eq!(p, vec![0x62, 0xF1, 0x90]),
            o => panic!("{:?}", o),
        }
    }
}
