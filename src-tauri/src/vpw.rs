#![allow(unused, dead_code)]
//! vpw.rs — J1850 VPW protocol layer for LS1 PCM (P01/P59)
//! Device IDs and frame format sourced from reference/VPW.cs (UniversalPatcher)
//! PCM  = 0x10, Tool = 0xF0, Broadcast = 0xFE
//!
//! Kernel modes (post Mode 34/36/37 upload):
//!   0x3C ReadBlock — bulk memory read while kernel is resident
//!   0x20 ExitKernel
//!   0x3F TestDevicePresent
//!   0xA0 HighSpeedPrepare / 0xA1 HighSpeed
//!
//! High-speed VPW (~4× normal bit rate) requires BOTH:
//!   1. Kernel accepting 0xA0 then 0xA1
//!   2. The tool/adapter switching its physical VPW timing
//! ELM327 clones often cannot do (2); J2534 / STN / AVT usually can.

use serialport::SerialPort;
use std::io::{Read, Write};

// ── Device addresses (from VPW.cs DeviceId) ───────────────────────────────
pub const PCM_ADDR: u8 = 0x10;
pub const TOOL_ADDR: u8 = 0xF0;
pub const BROADCAST_ADDR: u8 = 0xFE;

// ── Priority bytes (from VPW.cs Priority) ────────────────────────────────
pub const PRIO_HIGH_PHYS: u8 = 0x8C;
pub const PRIO_PHYS: u8 = 0x6C;
pub const PRIO_FUNC_OBD: u8 = 0x68;
pub const FUNC_TARGET: u8 = 0x6A;

// ── Mode constants (from VPW.cs Mode) ─────────────────────────────────
pub const MODE_EXIT_KERNEL: u8 = 0x20;
pub const MODE_READ_BLOCK: u8 = 0x3C;
pub const MODE_TEST_DEVICE: u8 = 0x3F;
pub const MODE_HS_PREPARE: u8 = 0xA0;
pub const MODE_HS_ENTER: u8 = 0xA1;

/// Positive response = mode | 0x40
pub const RESP_HS_PREPARE: u8 = 0xE0; // A0 + 0x40
pub const RESP_HS_ENTER: u8 = 0xE1;   // A1 + 0x40

// ── Frame builders ──────────────────────────────────────────

pub fn build_obd_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x01, pid];
    frame.push(vpw_crc8(&frame));
    frame
}

/// Mode 09 info request (VIN 0x02, CALID 0x04). Same VPW functional header as Mode 01.
pub fn build_mode09_request(info_type: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x09, info_type];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn parse_mode09_response(frame: &[u8], expected_info: u8) -> Option<Vec<u8>> {
    // Accept 49 <type> [count] <payload...>
    if frame.len() < 6 {
        return None;
    }
    for i in 0..frame.len().saturating_sub(2) {
        if frame[i] == 0x49 && frame[i + 1] == expected_info {
            let start = i + 2;
            let end = if validate_frame(frame) { frame.len() - 1 } else { frame.len() };
            if start < end {
                return Some(frame[start..end].to_vec());
            }
        }
    }
    None
}

pub fn ascii_from_obd_payload(data: &[u8]) -> String {
    let bytes = if data.first().copied().unwrap_or(0) <= 0x04 && data.len() > 1 {
        &data[1..]
    } else {
        data
    };
    let s: String = bytes
        .iter()
        .copied()
        .filter(|b| *b >= 0x20 && *b <= 0x7E)
        .map(|b| b as char)
        .collect();
    s.trim().to_string()
}

pub fn build_physical_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x01, pid];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_mode22_request(did_high: u8, did_low: u8) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x22, did_high, did_low];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_dtc_request() -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x03];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_clear_dtc_request() -> Vec<u8> {
    let mut frame = vec![PRIO_FUNC_OBD, FUNC_TARGET, TOOL_ADDR, 0x04];
    frame.push(vpw_crc8(&frame));
    frame
}

/// Mode 0x3C ReadBlock — kernel-assisted bulk memory read.
pub fn build_mode3c_read_block(address: u32, size: u16) -> Vec<u8> {
    let mut frame = vec![
        PRIO_HIGH_PHYS,
        PCM_ADDR,
        TOOL_ADDR,
        MODE_READ_BLOCK,
        ((address >> 16) & 0xFF) as u8,
        ((address >> 8) & 0xFF) as u8,
        (address & 0xFF) as u8,
        ((size >> 8) & 0xFF) as u8,
        (size & 0xFF) as u8,
    ];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_mode3f_test_device() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_TEST_DEVICE];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_mode20_exit_kernel() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_EXIT_KERNEL];
    frame.push(vpw_crc8(&frame));
    frame
}

/// Mode 0xA0 HighSpeedPrepare — tell kernel to arm high-speed VPW.
pub fn build_mode_a0_hs_prepare() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_HS_PREPARE];
    frame.push(vpw_crc8(&frame));
    frame
}

/// Mode 0xA1 HighSpeed — enter high-speed VPW (~4× bit rate).
/// Caller must also switch the tool/adapter physical layer after a positive E1.
pub fn build_mode_a1_hs_enter() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, MODE_HS_ENTER];
    frame.push(vpw_crc8(&frame));
    frame
}

// ── Checksum ────────────────────────────────────────────

/// Legacy additive byte. NOT the J1850 check byte; kept only so old callers and
/// tests can show the difference. No VPW builder uses it since v3.49.1.
pub fn vpw_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// SAE J1850 CRC-8 check byte appended to every outbound VPW frame. Same routine
/// the receive path uses (`elm_parse::j1850_crc8`), verified against real VPW
/// frames in reference/fixtures (e.g. `486B104100BFBFF990` -> `B5`).
pub fn vpw_crc8(data: &[u8]) -> u8 {
    crate::elm_parse::j1850_crc8(data)
}

/// A VPW frame is valid only when its last byte is the J1850 CRC-8 of the rest.
/// The additive byte is no longer accepted here (fail-closed since v3.49.1).
pub fn validate_frame(frame: &[u8]) -> bool {
    if frame.len() < 2 {
        return false;
    }
    let payload = &frame[..frame.len() - 1];
    vpw_crc8(payload) == frame[frame.len() - 1]
}

// ── Response parsers ────────────────────────────────────

pub fn parse_mode01_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 {
        return None;
    }
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_frame(frame) {
        return Some(frame[5..frame.len() - 1].to_vec());
    }
    None
}

pub fn parse_mode22_response(frame: &[u8], did_high: u8, did_low: u8) -> Option<Vec<u8>> {
    if frame.len() < 7 {
        return None;
    }
    if frame[3] == 0x62 && frame[4] == did_high && frame[5] == did_low && validate_frame(frame) {
        return Some(frame[6..frame.len() - 1].to_vec());
    }
    None
}

pub fn parse_dtc_response(frame: &[u8]) -> Option<Vec<[u8; 2]>> {
    if frame.len() < 5 || frame[3] != 0x43 || !validate_frame(frame) {
        return None;
    }
    let data = &frame[5..frame.len() - 1];
    Some(data.chunks_exact(2).map(|c| [c[0], c[1]]).collect())
}

pub fn parse_mode3c_response(frame: &[u8]) -> Result<Vec<u8>, String> {
    if frame.len() < 5 {
        return Err(format!("Mode 3C response too short: {} bytes", frame.len()));
    }
    let sid_idx = if frame.len() > 3 && (frame[3] == 0x7C || frame[3] == 0x7F) {
        3
    } else if frame.first() == Some(&0x7C) || frame.first() == Some(&0x7F) {
        0
    } else {
        return Err(format!(
            "Not a Mode 3C response (SID=0x{:02X})",
            frame.get(3).copied().unwrap_or(0)
        ));
    };

    if frame[sid_idx] == 0x7F {
        let nrc = frame.get(sid_idx + 2).copied().unwrap_or(0);
        return Err(format!("Mode 3C negative response NRC 0x{:02X}", nrc));
    }

    let mut data = frame[sid_idx + 1..].to_vec();
    if data.len() > 1 && validate_frame(frame) {
        data.pop();
    }
    Ok(data)
}

/// True if `frame` looks like a positive response to 0xA0 or 0xA1.
/// Accepts SID at index 0 or 3 (with/without 3-byte VPW header).
pub fn is_positive_hs_response(frame: &[u8], expected_resp: u8) -> bool {
    if frame.is_empty() {
        return false;
    }
    frame.iter().any(|&b| b == expected_resp)
        || frame.get(3).copied() == Some(expected_resp)
        || frame.first().copied() == Some(expected_resp)
}

/// Classify a high-speed control response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HsResponse {
    PrepareOk,
    EnterOk,
    Negative,
    Unknown,
}

pub fn parse_hs_response(frame: &[u8]) -> HsResponse {
    if frame.is_empty() {
        return HsResponse::Unknown;
    }
    if is_positive_hs_response(frame, RESP_HS_PREPARE) {
        return HsResponse::PrepareOk;
    }
    if is_positive_hs_response(frame, RESP_HS_ENTER) {
        return HsResponse::EnterOk;
    }
    // Generic negative: 0x7F present
    if frame.iter().any(|&b| b == 0x7F) {
        return HsResponse::Negative;
    }
    HsResponse::Unknown
}

// ── Serial I/O ─────────────────────────────────────────

/// Write a complete raw frame (header + data + CRC-8) as binary. Only for
/// genuine raw-VPW interfaces; ELM327/STN adapters take ASCII (see `request_response`).
pub fn send_frame(port: &mut Box<dyn SerialPort + Send>, frame: &[u8]) -> Result<(), String> {
    if frame.len() < 4 || !validate_frame(frame) {
        return Err("VPW send refused: frame has no valid J1850 CRC-8".into());
    }
    port.write_all(frame).map_err(|e| format!("VPW send error: {}", e))
}

pub fn recv_frame(port: &mut Box<dyn SerialPort + Send>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 1100];
    let n = port.read(&mut buf).map_err(|e| format!("VPW recv error: {}", e))?;
    if n == 0 {
        return Err("VPW recv: no data".to_string());
    }
    Ok(buf[..n].to_vec())
}

/// Binary request/response for raw-VPW interfaces (session protocol "vpw").
pub fn request_response_raw(
    port: &mut Box<dyn SerialPort + Send>,
    frame: &[u8],
) -> Result<Vec<u8>, String> {
    send_frame(port, frame)?;
    for _ in 0..4 {
        match recv_frame(port) {
            Ok(resp) if !resp.is_empty() => return Ok(resp),
            Ok(_) => continue,
            Err(e) if e.contains("no data") => continue,
            Err(e) => return Err(e),
        }
    }
    Err("VPW: no response after 4 attempts".to_string())
}

/// ELM327 datasheet: a non-CAN (J1850/ISO) request carries at most 7 data bytes
/// after the header set with AT SH. Longer frames need STN STPX (p01_backup::StnLink).
pub const ELM_J1850_MAX_DATA: usize = 7;

/// Split a sealed VPW frame into the ELM text form: `ATSH` header (3 bytes) and
/// the data bytes as hex. The CRC-8 is checked and dropped: the ELM/STN adapter
/// computes and appends the J1850 CRC itself.
pub fn elm_text_for_frame(frame: &[u8]) -> Result<(String, String), String> {
    if frame.len() < 5 {
        return Err(format!("VPW frame too short for ELM send: {} bytes", frame.len()));
    }
    if !validate_frame(frame) {
        return Err("VPW frame CRC-8 invalid; refusing to send".into());
    }
    let data = &frame[3..frame.len() - 1];
    if data.len() > ELM_J1850_MAX_DATA {
        return Err(format!(
            "VPW frame has {} data bytes; ELM327 J1850 sends at most {}. Use an STN adapter (STPX) for this request.",
            data.len(), ELM_J1850_MAX_DATA
        ));
    }
    let hex = |b: &[u8]| b.iter().map(|x| format!("{:02X}", x)).collect::<String>();
    Ok((hex(&frame[..3]), hex(data)))
}

/// Rebuild a raw frame (header + payload + CRC-8) from one parsed ELM line so the
/// frame parsers in this module work on either transport.
pub fn frame_from_elm_message(header: &str, payload: &[u8]) -> Option<Vec<u8>> {
    if header.len() != 6 {
        return None;
    }
    let mut f: Vec<u8> = (0..3).map(|i| u8::from_str_radix(&header[i * 2..i * 2 + 2], 16).ok()).collect::<Option<_>>()?;
    f.extend_from_slice(payload);
    f.push(vpw_crc8(&f));
    Some(f)
}

/// Pick the answer to `frame` from an ELM reply: a positive (mode | 0x40) or a
/// `7F <mode>` negative addressed to the tool.
pub fn pick_elm_answer(reply: &crate::elm_parse::ElmReply, frame: &[u8]) -> Option<Vec<u8>> {
    let mode = *frame.get(3)?;
    reply.messages.iter().find_map(|m| {
        let h = m.header.as_deref()?;
        let p = &m.payload;
        let hit = p.first() == Some(&(mode.wrapping_add(0x40))) || (p.first() == Some(&0x7F) && p.get(1) == Some(&mode));
        if hit { frame_from_elm_message(h, p) } else { None }
    })
}

/// Send a VPW frame over an ELM327/STN adapter as ASCII: AT SH with the frame
/// header, then the data bytes as hex (adapter adds the CRC). Refused unless the
/// adapter reports protocol 2 (SAE J1850 VPW, `ATDPN`, ELM327 datasheet).
/// The functional OBD header 68 6A F1 (ELM327 J1850 default) is restored on exit.
pub fn request_response_elm(
    port: &mut Box<dyn SerialPort + Send>,
    frame: &[u8],
) -> Result<Vec<u8>, String> {
    let (header, data) = elm_text_for_frame(frame)?;
    let dpn = crate::transport::elm_command(port, "ATDPN")?;
    if !elm_dpn_is_vpw(&dpn) {
        return Err(format!("Adapter is not on J1850 VPW (ATDPN: {}); VPW request refused", dpn.replace('>', "").trim()));
    }
    let mut g = crate::transport::AdapterGuard::new(port, &["ATSH686AF1"]);
    for c in ["ATH1", "ATS0"] {
        crate::transport::elm_command(g.port(), c)?;
    }
    crate::transport::elm_command(g.port(), &format!("ATSH{}", header))?;
    let mut sess = crate::transport::ElmSession::default();
    let reply = crate::transport::elm_request(g.port(), &mut sess, &data, crate::elm_parse::HeaderMode::ThreeByte, 2000)
        .map_err(|e| e.to_string())?;
    if reply.no_data {
        return Err("VPW: NO DATA".into());
    }
    pick_elm_answer(&reply, frame).ok_or_else(|| "VPW: no answer to this request in the adapter reply".into())
}

/// `ATDPN` answers e.g. `2` or `A2` (A = automatic). Protocol 2 is J1850 VPW.
pub fn elm_dpn_is_vpw(raw: &str) -> bool {
    raw.split(|c| c == '\r' || c == '\n')
        .map(|l| l.replace('>', "").trim().to_ascii_uppercase())
        .any(|l| l == "2" || l == "A2")
}

/// Request/response on whichever transport the session opened: raw binary for
/// raw-VPW interfaces, ASCII (AT SH + hex) for ELM327/STN adapters. Do not call
/// from inside `transport::with_session` (it reads the session protocol).
pub fn request_response(
    port: &mut Box<dyn SerialPort + Send>,
    frame: &[u8],
) -> Result<Vec<u8>, String> {
    let proto = crate::transport::current_protocol();
    if crate::transport::is_elm_path(&proto) {
        request_response_elm(port, frame)
    } else {
        request_response_raw(port, frame)
    }
}

// ── Flash / Kernel protocol builders ──────────────────────────────────

pub fn build_mode34_request(address: u32, size: u32) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x34, 0x00];
    frame.extend_from_slice(&address.to_be_bytes());
    frame.extend_from_slice(&size.to_be_bytes());
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_mode36_chunk(data: &[u8]) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x36];
    frame.extend_from_slice(data);
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_mode37_request() -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR, 0x37];
    frame.push(vpw_crc8(&frame));
    frame
}

pub fn build_physical_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![PRIO_HIGH_PHYS, PCM_ADDR, TOOL_ADDR];
    frame.extend_from_slice(payload);
    frame.push(vpw_crc8(&frame));
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode3c_frame_structure() {
        let f = build_mode3c_read_block(0x00020000, 0x0100);
        assert_eq!(f[3], MODE_READ_BLOCK);
        assert_eq!(f[4], 0x02);
        assert_eq!(f[5], 0x00);
        assert_eq!(f[6], 0x00);
        assert_eq!(f[7], 0x01);
        assert_eq!(f[8], 0x00);
        assert!(validate_frame(&f));
    }

    #[test]
    fn mode3c_response_parse() {
        let mut frame = vec![0x8C, 0xF0, 0x10, 0x7C, 0xAA, 0xBB, 0xCC, 0xDD];
        let cs = vpw_crc8(&frame);
        frame.push(cs);
        let data = parse_mode3c_response(&frame).unwrap();
        assert_eq!(data, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn exit_kernel_frame() {
        let f = build_mode20_exit_kernel();
        assert_eq!(f[3], MODE_EXIT_KERNEL);
        assert!(validate_frame(&f));
    }

    #[test]
    fn hs_prepare_enter_frames() {
        let a0 = build_mode_a0_hs_prepare();
        assert_eq!(a0[3], MODE_HS_PREPARE);
        assert!(validate_frame(&a0));
        let a1 = build_mode_a1_hs_enter();
        assert_eq!(a1[3], MODE_HS_ENTER);
        assert!(validate_frame(&a1));
    }

    #[test]
    fn hs_response_parse() {
        let prep = vec![0x8C, 0xF0, 0x10, RESP_HS_PREPARE, 0x00];
        assert_eq!(parse_hs_response(&prep), HsResponse::PrepareOk);
        let enter = vec![0x8C, 0xF0, 0x10, RESP_HS_ENTER];
        assert_eq!(parse_hs_response(&enter), HsResponse::EnterOk);
        let neg = vec![0x8C, 0xF0, 0x10, 0x7F, 0xA0, 0x11];
        assert_eq!(parse_hs_response(&neg), HsResponse::Negative);
    }
}

#[cfg(test)]
mod j1850_crc_tests {
    use super::*;
    #[test]
    fn real_vpw_frame_with_crc_parses() {
        // Real reply 486B10 41 05 7B + CRC 06 (ECT 83 C).
        let f = [0x48, 0x6B, 0x10, 0x41, 0x05, 0x7B, 0x06];
        assert!(validate_frame(&f));
        assert_eq!(parse_mode01_response(&f, 0x05), Some(vec![0x7B]));
        assert!(!validate_frame(&[0x48, 0x6B, 0x10, 0x41, 0x05, 0x7B, 0x07]));
    }
}

#[cfg(test)]
mod tx_crc_tests {
    use super::*;

    /// Request frames from reference/fixtures/elm_patrol_*? No: VPW TX vectors come
    /// from the same CRC routine verified on real RX frames. Here the real fixture
    /// reply 486B10 4100BFBFF990 -> B5 is rebuilt through the TX sealer.
    #[test]
    fn sealer_matches_real_fixture_crc() {
        let f = frame_from_elm_message("486B10", &[0x41, 0x00, 0xBF, 0xBF, 0xF9, 0x90]).unwrap();
        assert_eq!(*f.last().unwrap(), 0xB5);
        assert!(validate_frame(&f));
    }

    #[test]
    fn builders_append_crc8_not_additive() {
        for f in [build_obd_request(0x0C), build_mode09_request(0x02), build_mode3c_read_block(0x8000, 64),
                  build_mode3f_test_device(), build_mode20_exit_kernel(), build_physical_frame(&[0x3C, 0x0A])] {
            let body = &f[..f.len() - 1];
            assert_eq!(*f.last().unwrap(), crate::elm_parse::j1850_crc8(body));
            assert!(validate_frame(&f));
        }
        // 68 6A F0 01 0C: additive would be 0xD7; must not validate as additive.
        let mut bad = vec![0x68, 0x6A, 0xF0, 0x01, 0x0C];
        bad.push(vpw_checksum(&bad));
        assert!(!validate_frame(&bad));
    }

    #[test]
    fn elm_text_strips_crc_and_splits_header() {
        let (h, d) = elm_text_for_frame(&build_obd_request(0x0C)).unwrap();
        assert_eq!(h, "686AF0");
        assert_eq!(d, "010C");
        let (h, d) = elm_text_for_frame(&build_mode3c_read_block(0x008000, 0x40)).unwrap();
        assert_eq!(h, "8C10F0");
        assert_eq!(d, "3C0080000040");
    }

    #[test]
    fn elm_text_refuses_bad_crc_and_long_frames() {
        let mut f = build_obd_request(0x0C);
        *f.last_mut().unwrap() ^= 1;
        assert!(elm_text_for_frame(&f).is_err());
        let long = build_mode36_chunk(&[0u8; 8]);
        assert!(elm_text_for_frame(&long).unwrap_err().contains("STN"));
    }

    #[test]
    fn picks_answer_from_real_elm_reply() {
        let reply = crate::elm_parse::parse_reply("486B104105 7B06\r\r>", crate::elm_parse::HeaderMode::ThreeByte);
        let f = pick_elm_answer(&reply, &build_obd_request(0x05)).unwrap();
        assert_eq!(parse_mode01_response(&f, 0x05), Some(vec![0x7B]));
        assert!(pick_elm_answer(&reply, &build_obd_request(0x0C)).is_some()); // mode 01 match only
        assert!(pick_elm_answer(&reply, &build_mode09_request(0x02)).is_none());
    }

    #[test]
    fn dpn_vpw_detection() {
        assert!(elm_dpn_is_vpw("A2\r\r>"));
        assert!(elm_dpn_is_vpw("2\r>"));
        assert!(!elm_dpn_is_vpw("A6\r>"));
        assert!(!elm_dpn_is_vpw("?\r>"));
    }
}
