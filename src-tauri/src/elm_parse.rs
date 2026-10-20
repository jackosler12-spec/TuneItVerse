//! elm_parse.rs — line-based parser for ELM327 / STN (OBDLink) text replies.
//!
//! The adapter prints one CAN/J1850 frame per line. With `ATH1` each line starts
//! with the header (3 hex digits for 11-bit CAN, 8 for 29-bit CAN, 3 bytes for
//! J1850/ISO 9141/KWP), so joining every hex digit and cutting pairs shifts every
//! byte (`7E804410C0BBC` → `7E 80 44 ..`). This module parses each line on its
//! own, strips the header, follows the ISO-TP PCI byte (single / first /
//! consecutive frames) and reports `NO DATA`, adapter errors and `7F` negative
//! responses separately so none of them can be mistaken for success.
//!
//! Real reference replies (OBDLink MX+, ATH1 ATS0, 2007 Patrol ZD30CRD) live in
//! `reference/fixtures/elm_patrol_zd30_obdlink.txt` and drive the unit tests.

use std::collections::BTreeMap;

/// How the adapter prints headers. `Auto` infers per line (see `split_header`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderMode {
    /// `ATH0`: no header, PCI hidden for single frames, multi-frame as `0:`/`1:` lines.
    Off,
    /// 11-bit CAN, 3 hex digit header (`7E8`).
    Can11,
    /// 29-bit CAN, 8 hex digit header (`18DAF110`).
    Can29,
    /// J1850 / ISO 9141 / KWP: 3 header bytes in front, 1 check byte at the end.
    ThreeByte,
    /// Infer per line.
    Auto,
}

/// One assembled ECU message (service byte first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcuMessage {
    /// Header text as printed (`7E8`, `486B10`), `None` with headers off.
    pub header: Option<String>,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElmReply {
    pub messages: Vec<EcuMessage>,
    pub no_data: bool,
    /// Adapter-level failures (`CAN ERROR`, `STOPPED`, `BUFFER FULL`, `?` ...).
    pub adapter_errors: Vec<String>,
    /// Lines that were not frames and not errors (`SEARCHING...`, `OK`, banners).
    pub noise: Vec<String>,
    pub searching: bool,
    /// Frames that could not be assembled (bad PCI, missing consecutive frame).
    pub malformed: Vec<String>,
    /// Header of every CAN line seen, including malformed ones (for foreign-frame checks).
    pub headers_seen: Vec<String>,
    /// Headers whose answer for this SID cannot be trusted (an earlier request with the
    /// same SID was left pending on that ECU, so a late answer could land here).
    pub ambiguous: Vec<(u8, String)>,
}

/// Why a request produced no usable data. None of these count as success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElmError {
    NoData,
    /// `7F <sid> <nrc>` with an NRC other than 0x78.
    Negative { sid: u8, nrc: u8 },
    /// Only `7F <sid> 78` (response pending) arrived and nothing after it.
    Pending { sid: u8 },
    Adapter(String),
    /// Frames arrived but none answered the request.
    Unmatched(String),
    /// A positive answer arrived from an ECU that still owed a late answer to an
    /// earlier request with the same SID; it cannot be attributed safely.
    Ambiguous { sid: u8, header: String },
    Io(String),
}

impl std::fmt::Display for ElmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElmError::NoData => write!(f, "NO DATA"),
            ElmError::Negative { sid, nrc } => write!(f, "negative response 7F {:02X} {:02X} ({})", sid, nrc, nrc_name(*nrc)),
            ElmError::Pending { sid } => write!(f, "7F {:02X} 78 response pending, no final answer", sid),
            ElmError::Adapter(s) => write!(f, "adapter error: {}", s),
            ElmError::Unmatched(s) => write!(f, "no matching response: {}", s),
            ElmError::Ambiguous { sid, header } => write!(f, "answer from {} to SID {:02X} is ambiguous (earlier request still pending on that ECU); discarded", header, sid),
            ElmError::Io(s) => write!(f, "{}", s),
        }
    }
}

/// ISO 14229 / ISO 14230 NRC names (public standard values).
pub fn nrc_name(nrc: u8) -> &'static str {
    match nrc {
        0x10 => "generalReject",
        0x11 => "serviceNotSupported",
        0x12 => "subFunctionNotSupported",
        0x13 => "incorrectMessageLength",
        0x21 => "busyRepeatRequest",
        0x22 => "conditionsNotCorrect",
        0x24 => "requestSequenceError",
        0x31 => "requestOutOfRange",
        0x33 => "securityAccessDenied",
        0x35 => "invalidKey",
        0x36 => "exceedNumberOfAttempts",
        0x37 => "requiredTimeDelayNotExpired",
        0x78 => "responsePending",
        0x7E => "subFunctionNotSupportedInActiveSession",
        0x7F => "serviceNotSupportedInActiveSession",
        _ => "unknown NRC",
    }
}

const ADAPTER_ERRORS: &[&str] = &[
    "CAN ERROR", "BUS ERROR", "BUS BUSY", "BUFFER FULL", "STOPPED", "UNABLE TO CONNECT",
    "FB ERROR", "DATA ERROR", "RX ERROR", "<RX ERROR", "ERR", "ACT ALERT", "LV RESET",
    "LP ALERT", "BUS INIT: ...ERROR", "BUS INIT:...ERROR",
];

fn is_hex_line(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// SAE J1850 CRC-8 (poly 0x1D, init 0xFF, final XOR 0xFF). Checked against real
/// VPW frames in the fixture (e.g. `486B104100BFBFF990` → `B5`).
pub fn j1850_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0xFF;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x1D } else { crc << 1 };
        }
    }
    crc ^ 0xFF
}

fn additive8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, &b| a.wrapping_add(b))
}

enum Line {
    /// CAN frame: header + raw bytes from PCI onwards.
    Can { header: Option<String>, bytes: Vec<u8> },
    /// Non-CAN frame (header + payload, check byte removed).
    Plain { header: Option<String>, payload: Vec<u8> },
    /// ATH0 multi-frame segment `N:` line.
    Segment { index: u8, bytes: Vec<u8> },
    /// ATH0 multi-frame total-length line (3 hex digits, e.g. `014`).
    Length(usize),
}

fn classify_line(line: &str, mode: HeaderMode) -> Option<Line> {
    // ATH0 multi-frame layout: "014" then "0: 49 02 01 ..." "1: ...".
    if let Some((idx, rest)) = line.split_once(':') {
        if idx.len() == 1 && idx.chars().all(|c| c.is_ascii_hexdigit()) && is_hex_line(rest) {
            let index = u8::from_str_radix(idx, 16).ok()?;
            return Some(Line::Segment { index, bytes: hex_bytes(rest)? });
        }
        return None;
    }
    if !is_hex_line(line) {
        return None;
    }
    let mode = match mode {
        HeaderMode::Auto => infer_mode(line),
        m => m,
    };
    match mode {
        HeaderMode::Can11 => {
            if line.len() < 5 || line.len() % 2 == 0 {
                return None;
            }
            Some(Line::Can { header: Some(line[..3].to_string()), bytes: hex_bytes(&line[3..])? })
        }
        HeaderMode::Can29 => {
            if line.len() < 10 || line.len() % 2 != 0 {
                return None;
            }
            Some(Line::Can { header: Some(line[..8].to_string()), bytes: hex_bytes(&line[8..])? })
        }
        HeaderMode::ThreeByte => {
            let b = hex_bytes(line)?;
            if b.len() < 5 {
                return None;
            }
            let (body, check) = b.split_at(b.len() - 1);
            if j1850_crc8(body) != check[0] && additive8(body) != check[0] {
                return None;
            }
            Some(Line::Plain { header: Some(line[..6].to_string()), payload: body[3..].to_vec() })
        }
        HeaderMode::Off | HeaderMode::Auto => {
            if line.len() == 3 {
                return usize::from_str_radix(line, 16).ok().map(Line::Length);
            }
            Some(Line::Plain { header: None, payload: hex_bytes(line)? })
        }
    }
}

/// Per-line header guess when the caller does not know the protocol:
/// odd length → 11-bit CAN; `18DA`/`18DB` prefix → 29-bit CAN; an even line
/// whose last byte is a valid J1850 CRC or additive checksum → 3-byte header;
/// otherwise headers are assumed off.
fn infer_mode(line: &str) -> HeaderMode {
    if line.len() % 2 == 1 {
        return if line.len() == 3 { HeaderMode::Off } else { HeaderMode::Can11 };
    }
    if line.len() >= 10 && (line.starts_with("18DA") || line.starts_with("18DB")) {
        return HeaderMode::Can29;
    }
    if let Some(b) = hex_bytes(line) {
        if b.len() >= 5 {
            let (body, check) = b.split_at(b.len() - 1);
            if j1850_crc8(body) == check[0] || additive8(body) == check[0] {
                return HeaderMode::ThreeByte;
            }
        }
    }
    HeaderMode::Off
}

/// Parse one complete adapter reply (everything up to the `>` prompt).
pub fn parse_reply(raw: &str, mode: HeaderMode) -> ElmReply {
    let mut out = ElmReply::default();
    // ISO-TP reassembly per header: (expected_len, next_seq, data)
    let mut pending_mf: BTreeMap<String, (usize, u8, Vec<u8>)> = BTreeMap::new();
    let mut seg_total: Option<usize> = None;
    let mut seg_data: Vec<u8> = Vec::new();
    let mut seg_next: u8 = 0;

    for raw_line in raw.split(|c| c == '\r' || c == '\n') {
        let line_up = raw_line.replace('>', "").trim().to_ascii_uppercase();
        if line_up.is_empty() {
            continue;
        }
        let compact: String = line_up.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.contains("NODATA") {
            out.no_data = true;
            continue;
        }
        if compact.starts_with("SEARCHING") {
            out.searching = true;
            // OBDLink can print the first frame on the same line after "SEARCHING...".
            let rest = compact.trim_start_matches("SEARCHING").trim_start_matches('.');
            if rest.is_empty() {
                continue;
            }
            feed_line(rest, mode, &mut out, &mut pending_mf, &mut seg_total, &mut seg_data, &mut seg_next);
            continue;
        }
        if compact == "?" || ADAPTER_ERRORS.iter().any(|e| line_up.contains(e) && !is_hex_line(&compact)) {
            out.adapter_errors.push(line_up.clone());
            continue;
        }
        feed_line(&compact, mode, &mut out, &mut pending_mf, &mut seg_total, &mut seg_data, &mut seg_next);
    }
    for (h, (len, _, data)) in pending_mf {
        out.malformed.push(format!("{}: multi-frame incomplete ({} of {} bytes)", h, data.len(), len));
    }
    if let Some(total) = seg_total {
        if seg_data.len() >= total {
            seg_data.truncate(total);
            out.messages.push(EcuMessage { header: None, payload: seg_data });
        } else {
            out.malformed.push(format!("multi-frame incomplete ({} of {} bytes)", seg_data.len(), total));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn feed_line(
    compact: &str,
    mode: HeaderMode,
    out: &mut ElmReply,
    pending_mf: &mut BTreeMap<String, (usize, u8, Vec<u8>)>,
    seg_total: &mut Option<usize>,
    seg_data: &mut Vec<u8>,
    seg_next: &mut u8,
) {
    match classify_line(compact, mode) {
        None => out.noise.push(compact.to_string()),
        Some(Line::Length(n)) => {
            *seg_total = Some(n);
            seg_data.clear();
            *seg_next = 0;
        }
        Some(Line::Segment { index, bytes }) => {
            if seg_total.is_none() || index != *seg_next {
                out.malformed.push(format!("segment {} out of order", index));
                return;
            }
            seg_data.extend_from_slice(&bytes);
            *seg_next = (index + 1) & 0x0F;
        }
        Some(Line::Plain { header, payload }) => {
            if !payload.is_empty() {
                out.messages.push(EcuMessage { header, payload });
            }
        }
        Some(Line::Can { header, bytes }) => {
            let key = header.clone().unwrap_or_default();
            if !key.is_empty() {
                out.headers_seen.push(key.clone());
            }
            let Some(&pci) = bytes.first() else { return };
            match pci >> 4 {
                0x0 => {
                    let len = (pci & 0x0F) as usize;
                    if len == 0 || bytes.len() < 1 + len {
                        out.malformed.push(compact.to_string());
                        return;
                    }
                    out.messages.push(EcuMessage { header, payload: bytes[1..1 + len].to_vec() });
                }
                0x1 => {
                    if bytes.len() < 3 {
                        out.malformed.push(compact.to_string());
                        return;
                    }
                    let len = (((pci & 0x0F) as usize) << 8) | bytes[1] as usize;
                    pending_mf.insert(key, (len, 1, bytes[2..].to_vec()));
                }
                0x2 => {
                    let seq = pci & 0x0F;
                    match pending_mf.get_mut(&key) {
                        Some((len, next, data)) if *next == seq => {
                            data.extend_from_slice(&bytes[1..]);
                            *next = (seq + 1) & 0x0F;
                            if data.len() >= *len {
                                let len = *len;
                                let mut data = pending_mf.remove(&key).map(|t| t.2).unwrap_or_default();
                                data.truncate(len);
                                out.messages.push(EcuMessage { header, payload: data });
                            }
                        }
                        _ => out.malformed.push(format!("{}: unexpected consecutive frame", compact)),
                    }
                }
                0x3 => out.noise.push(compact.to_string()), // flow control echo
                _ => out.malformed.push(compact.to_string()),
            }
        }
    }
}

fn is_pending(m: &EcuMessage, sid: u8) -> bool {
    m.payload.len() >= 3 && m.payload[0] == 0x7F && m.payload[1] == sid && m.payload[2] == 0x78
}

fn answers_sid(m: &EcuMessage, sid: u8) -> bool {
    m.payload.first() == Some(&sid.wrapping_add(0x40)) || (m.payload.len() >= 2 && m.payload[0] == 0x7F && m.payload[1] == sid)
}

/// ISO 15765-4 OBD response IDs: 11-bit 7E8..7EF, 29-bit 18DAF1xx.
pub fn is_obd_response_header(h: &str) -> bool {
    let h = h.to_ascii_uppercase();
    (h.len() == 3 && h.starts_with("7E") && matches!(h.as_bytes()[2], b'8'..=b'9' | b'A'..=b'F'))
        || (h.len() == 8 && h.starts_with("18DAF1"))
}

impl ElmReply {
    /// True when only `7F <sid> 78` arrived for `sid` and no final answer followed.
    pub fn only_pending(&self, sid: u8) -> bool {
        let pending = self.messages.iter().any(|m| is_pending(m, sid));
        pending && !self.messages.iter().any(|m| !is_pending(m, sid))
    }

    /// ECUs (headers) that sent `7F <sid> 78` and no final answer for `sid` yet.
    pub fn pending_headers(&self, sid: u8) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in self.messages.iter().filter(|m| is_pending(m, sid)) {
            let h = m.header.clone().unwrap_or_default();
            let done = self.messages.iter().any(|o| o.header == m.header && answers_sid(o, sid) && !is_pending(o, sid));
            if !done && !out.contains(&h) {
                out.push(h);
            }
        }
        out
    }

    /// CAN frames from IDs that are not OBD responders (e.g. 2D1/23D broadcast
    /// traffic leaking through after a mask/filter change).
    pub fn foreign_headers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for h in &self.headers_seen {
            if !is_obd_response_header(h) && !out.contains(h) {
                out.push(h.clone());
            }
        }
        out
    }

    fn is_ambiguous(&self, m: &EcuMessage, sid: u8) -> bool {
        let h = m.header.clone().unwrap_or_default();
        self.ambiguous.iter().any(|(s, a)| *s == sid && *a == h)
    }

    /// Positive answer to service `sid` whose payload starts with `sid+0x40` then `prefix`.
    /// Returns the bytes after the prefix. Negative responses and NO DATA are errors.
    pub fn positive(&self, sid: u8, prefix: &[u8]) -> Result<Vec<u8>, ElmError> {
        self.positive_from(None, sid, prefix)
    }

    /// Like `positive`, restricted to one responder header (e.g. `7E8`) when given.
    /// The SID echo (sid+0x40) and `prefix` (PID/DID echo) must match, so a stray
    /// frame for another service (e.g. a late `59 02 FF`) is never taken as the answer.
    pub fn positive_from(&self, header: Option<&str>, sid: u8, prefix: &[u8]) -> Result<Vec<u8>, ElmError> {
        let pos = sid.wrapping_add(0x40);
        let of_header = |m: &&EcuMessage| header.map(|h| m.header.as_deref() == Some(h)).unwrap_or(true);
        let matches = |m: &EcuMessage| m.payload.first() == Some(&pos) && m.payload.len() >= 1 + prefix.len() && &m.payload[1..1 + prefix.len()] == prefix;
        let mut ambiguous_hit: Option<String> = None;
        for m in self.messages.iter().filter(of_header) {
            if matches(m) {
                if self.is_ambiguous(m, sid) {
                    ambiguous_hit.get_or_insert(m.header.clone().unwrap_or_default());
                    continue;
                }
                return Ok(m.payload[1 + prefix.len()..].to_vec());
            }
        }
        for m in self.messages.iter().filter(of_header) {
            if m.payload.len() >= 3 && m.payload[0] == 0x7F && m.payload[1] == sid && m.payload[2] != 0x78 && !self.is_ambiguous(m, sid) {
                return Err(ElmError::Negative { sid, nrc: m.payload[2] });
            }
        }
        if let Some(h) = ambiguous_hit {
            return Err(ElmError::Ambiguous { sid, header: h });
        }
        let pend = self.pending_headers(sid);
        if pend.iter().any(|h| header.map(|w| w == h).unwrap_or(true)) {
            return Err(ElmError::Pending { sid });
        }
        if let Some(e) = self.adapter_errors.first() {
            return Err(ElmError::Adapter(e.clone()));
        }
        if self.no_data {
            return Err(ElmError::NoData);
        }
        if self.messages.is_empty() && self.malformed.is_empty() {
            return Err(ElmError::NoData);
        }
        Err(ElmError::Unmatched(format!("{} message(s), {} malformed", self.messages.len(), self.malformed.len())))
    }
}

/// Decode an OBD/Nissan-style 4-byte "supported" bitmask: MSB of byte 0 is
/// `base+1`, LSB of byte 3 is `base+0x20`.
pub fn decode_support_mask(base: u16, mask: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    for (i, byte) in mask.iter().take(4).enumerate() {
        for bit in 0..8 {
            if byte & (0x80 >> bit) != 0 {
                out.push(base + (i as u16) * 8 + bit as u16 + 1);
            }
        }
    }
    out
}

/// Legacy helper: every hex byte in a header-less text blob (used for user-typed
/// hex such as the STPX box). Do NOT use it on adapter replies with headers on.
pub fn hex_from_text(text: &str) -> Vec<u8> {
    let cleaned: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..cleaned.len() / 2).filter_map(|i| u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(raw: &str) -> ElmReply {
        parse_reply(&raw.replace(" | ", "\r"), HeaderMode::Auto)
    }

    #[test]
    fn real_patrol_single_frames_decode() {
        // Real OBDLink MX+ replies, ATH1 ATS0, 11-bit CAN.
        assert_eq!(p("7E804410C0BBC\r\r>").positive(0x01, &[0x0C]).unwrap(), vec![0x0B, 0xBC]);
        assert_eq!(p("7E80441230CF6").positive(0x01, &[0x23]).unwrap(), vec![0x0C, 0xF6]);
        assert_eq!(p("7E803410550").positive(0x01, &[0x05]).unwrap(), vec![0x50]);
        assert_eq!(p("7E806410098390011").positive(0x01, &[0x00]).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
        assert_eq!(p("7E8064120A0000000").positive(0x01, &[0x20]).unwrap(), vec![0xA0, 0, 0, 0]);
        assert_eq!(p("7E80250C0").positive(0x10, &[0xC0]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn explicit_can11_mode_matches_auto() {
        let r = parse_reply("7E804410C0BBC", HeaderMode::Can11);
        assert_eq!(r.positive(0x01, &[0x0C]).unwrap(), vec![0x0B, 0xBC]);
    }

    #[test]
    fn spaced_headers_parse() {
        let r = p("7E8 04 41 0C 0B BC \r>");
        assert_eq!(r.positive(0x01, &[0x0C]).unwrap(), vec![0x0B, 0xBC]);
    }

    #[test]
    fn searching_prefix_is_noise_not_data() {
        let r = p("SEARCHING... | 7E806410098390011");
        assert!(r.searching);
        assert_eq!(r.positive(0x01, &[0x00]).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
        let r = p("SEARCHING...7E806410098390011\r>");
        assert_eq!(r.positive(0x01, &[0x00]).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
    }

    #[test]
    fn no_data_is_an_error() {
        assert_eq!(p("NO DATA\r\r>").positive(0x01, &[0x0C]), Err(ElmError::NoData));
        assert_eq!(p("SEARCHING...\rNO DATA\r>").positive(0x01, &[0x0C]), Err(ElmError::NoData));
        assert_eq!(p(">").positive(0x01, &[0x0C]), Err(ElmError::NoData));
        assert_eq!(p("").positive(0x01, &[0x0C]), Err(ElmError::NoData));
    }

    #[test]
    fn adapter_errors_are_errors() {
        for e in ["CAN ERROR", "STOPPED", "BUFFER FULL", "UNABLE TO CONNECT", "BUS INIT: ...ERROR", "?", "<RX ERROR"] {
            let r = p(&format!("{}\r>", e));
            assert!(matches!(r.positive(0x01, &[0x0C]), Err(ElmError::Adapter(_))), "{e}");
        }
    }

    #[test]
    fn text_noise_never_leaks_hex() {
        // "SEARCHING", "DATA", "BUFFER" all contain hex letters; old parser leaked them.
        let r = p("SEARCHING...\rBUS INIT...\rOK\r7E803410550\r>");
        assert_eq!(r.messages.len(), 1);
        assert_eq!(r.positive(0x01, &[0x05]).unwrap(), vec![0x50]);
    }

    #[test]
    fn negative_and_pending() {
        // Real: 221209 -> 7F2212
        assert_eq!(p("7E8037F2212").positive(0x22, &[0x12, 0x09]), Err(ElmError::Negative { sid: 0x22, nrc: 0x12 }));
        // Pending alone keeps the request open.
        assert_eq!(p("7E8037F0978").positive(0x09, &[0x02]), Err(ElmError::Pending { sid: 0x09 }));
        // Real 0902: pending then multi-frame answer (VIN all zero on this ECU).
        let r = p("7E8037F0978 | 7E81014490201000000 | 7E82100000000000000 | 7E82200000000000000");
        let vin = r.positive(0x09, &[0x02]).unwrap();
        assert_eq!(vin.len(), 18);
        assert_eq!(vin[0], 0x01);
        assert!(vin[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn real_multiframe_calid() {
        let r = p("7E81013490401305653 | 7E82134334200000000 | 7E82200000000000000");
        let d = r.positive(0x09, &[0x04]).unwrap();
        assert_eq!(d[0], 0x01);
        let s: String = d[1..].iter().filter(|b| (0x20..0x7F).contains(*b)).map(|&b| b as char).collect();
        assert_eq!(s, "0VS43B");
    }

    #[test]
    fn multiframe_out_of_order_is_malformed() {
        let r = p("7E81013490401305653 | 7E82200000000000000");
        assert!(r.positive(0x09, &[0x04]).is_err());
        assert!(!r.malformed.is_empty());
    }

    #[test]
    fn headers_off_multiframe() {
        let r = parse_reply("014\r0: 49 02 01 31 47 31\r1: 4A 43 35 34 34 34 52\r2: 37 32 35 32 33 36 37\r>", HeaderMode::Off);
        let d = r.positive(0x09, &[0x02]).unwrap();
        assert_eq!(d.len(), 18);
        let r = parse_reply("41 0C 0B BC\r>", HeaderMode::Off);
        assert_eq!(r.positive(0x01, &[0x0C]).unwrap(), vec![0x0B, 0xBC]);
    }

    #[test]
    fn can29_header() {
        let r = parse_reply("18DAF11004410C0BBC", HeaderMode::Auto);
        assert_eq!(r.positive(0x01, &[0x0C]).unwrap(), vec![0x0B, 0xBC]);
    }

    #[test]
    fn real_vpw_frames_with_crc() {
        // Chevrolet J1850 VPW session (ATDPN=A2), headers on, CRC byte shown.
        let r = p("486B104100BFBFF990B5");
        assert_eq!(r.messages[0].header.as_deref(), Some("486B10"));
        assert_eq!(r.positive(0x01, &[0x00]).unwrap(), vec![0xBF, 0xBF, 0xF9, 0x90]);
        assert_eq!(p("486B1041057B06").positive(0x01, &[0x05]).unwrap(), vec![0x7B]);
        assert_eq!(p("486B10410100002000E9").positive(0x01, &[0x01]).unwrap(), vec![0x00, 0x00, 0x20, 0x00]);
        assert_eq!(j1850_crc8(&[0x48, 0x6B, 0x10, 0x41, 0x04, 0x01]), 0xC1);
        // Corrupted CRC under explicit 3-byte mode is rejected.
        assert!(parse_reply("486B104100BFBFF99000", HeaderMode::ThreeByte).positive(0x01, &[0x00]).is_err());
    }

    #[test]
    fn support_mask_decodes_patrol() {
        assert_eq!(decode_support_mask(0x00, &[0x98, 0x39, 0x00, 0x11]), vec![0x01, 0x04, 0x05, 0x0B, 0x0C, 0x0D, 0x10, 0x1C, 0x20]);
        assert_eq!(decode_support_mask(0x20, &[0xA0, 0, 0, 0]), vec![0x21, 0x23]);
    }

    #[test]
    fn multi_ecu_pending_and_foreign() {
        // Real (non-Patrol, multi-ECU car): 7E8 pending, 7E9/7EB answered before the prompt.
        let r = p("7E8037F1978 | 7EB035902FF | 7E9035902FF");
        assert_eq!(r.pending_headers(0x19), vec!["7E8".to_string()]);
        assert_eq!(r.positive_from(Some("7E8"), 0x19, &[0x02]), Err(ElmError::Pending { sid: 0x19 }));
        assert_eq!(r.positive_from(Some("7E9"), 0x19, &[0x02]).unwrap(), vec![0xFF]);
        assert!(!r.only_pending(0x19));
        assert!(r.foreign_headers().is_empty());
        // Real 11 Aug flood after STCAF0 + ATCM000: 2D1/23D are foreign, never data.
        let r = p("23D03000000 | 7E806410098390011 | 2D1200000030E");
        assert_eq!(r.foreign_headers(), vec!["23D".to_string(), "2D1".to_string()]);
        assert_eq!(r.positive(0x01, &[0x00]).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
        let r = p("2D1200000030E");
        assert!(r.positive(0x01, &[0x00]).is_err());
        assert_eq!(r.foreign_headers(), vec!["2D1".to_string()]);
    }

    #[test]
    fn stray_answer_for_other_service_is_not_taken() {
        let r = p("7E8035902FF");
        assert!(matches!(r.positive(0x01, &[0x00]), Err(ElmError::Unmatched(_))));
        let r = p("7E8035902FF | 7E806410098390011");
        assert_eq!(r.positive(0x01, &[0x00]).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
    }

    #[test]
    fn single_frame_bad_length_is_malformed() {
        let r = p("7E806410C0B");
        assert!(r.positive(0x01, &[0x0C]).is_err());
    }
}
