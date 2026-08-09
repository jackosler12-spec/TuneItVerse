//! uds.rs — ISO 14229 Unified Diagnostic Services (application layer)
//!
//! Transport-agnostic service builders + high-level helpers that ride on
//! `can::uds_request` / `can::uds_request_multiframe` (ISO-TP).
//!
//! Covers the gaps identified for TuneItVerse v2.0:
//!   0x10 DiagnosticSessionControl
//!   0x3E TesterPresent (keep-alive)
//!   0x22 ReadDataByIdentifier
//!   0x23 ReadMemoryByAddress (configurable ALFI)
//!   0x27 SecurityAccess (payload helpers; keys in security.rs)
//!   0x28 CommunicationControl
//!   0x31 RoutineControl
//!   0x34 / 0x36 / 0x37 RequestDownload / TransferData / TransferExit
//!   0x19 ReadDTCInformation (reportNumberOfDTCByStatusMask / reportDTCByStatusMask)
//!   0x14 ClearDiagnosticInformation
//!   0x85 ControlDTCSetting
//!   0x11 ECUReset
//!
//! Negative responses: 0x7F + SID + NRC with human-readable mapping.

#![allow(dead_code)]

use serialport::SerialPort;
use std::time::{Duration, Instant};

// ── Service IDs ─────────────────────────────────────────────────────────────

pub const SID_DSC: u8 = 0x10;
pub const SID_ECU_RESET: u8 = 0x11;
pub const SID_CLEAR_DTC: u8 = 0x14;
pub const SID_READ_DTC: u8 = 0x19;
pub const SID_RDBI: u8 = 0x22;
pub const SID_RMBA: u8 = 0x23;
pub const SID_SA: u8 = 0x27;
pub const SID_CC: u8 = 0x28;
pub const SID_WDBI: u8 = 0x2E;
pub const SID_IOCBI: u8 = 0x2F;
pub const SID_RC: u8 = 0x31;
pub const SID_RD: u8 = 0x34;
pub const SID_RU: u8 = 0x35;
pub const SID_TD: u8 = 0x36;
pub const SID_RTE: u8 = 0x37;
pub const SID_TP: u8 = 0x3E;
pub const SID_CDTCS: u8 = 0x85;

// ── Sessions ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticSession {
    Default = 0x01,
    Programming = 0x02,
    Extended = 0x03,
    SafetySystem = 0x04,
}

impl DiagnosticSession {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Default),
            0x02 => Some(Self::Programming),
            0x03 => Some(Self::Extended),
            0x04 => Some(Self::SafetySystem),
            _ => None,
        }
    }
}

// ── AddressAndLengthFormatIdentifier (ALFI) for 0x23 / 0x34 ─────────────────

/// High nibble = size-of-memorySize, low nibble = size-of-memoryAddress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alfi(pub u8);

impl Alfi {
    /// 4-byte address + 2-byte size (common Bosch / ISO default for Mode 23).
    pub const ADDR4_SIZE2: Alfi = Alfi(0x24);
    /// Explicit 4-byte addr + 2-byte size as used in current flash path packing.
    pub const PACKED_42: Alfi = Alfi(0x42);
    /// 4-byte address + 4-byte size (large downloads).
    pub const ADDR4_SIZE4: Alfi = Alfi(0x44);
    /// 3-byte address + 2-byte size.
    pub const ADDR3_SIZE2: Alfi = Alfi(0x23);

    pub fn address_bytes(self) -> usize {
        (self.0 & 0x0F) as usize
    }
    pub fn size_bytes(self) -> usize {
        ((self.0 >> 4) & 0x0F) as usize
    }
}

// ── Negative response ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegativeResponse {
    pub sid: u8,
    pub nrc: u8,
}

impl NegativeResponse {
    pub fn description(&self) -> &'static str {
        nrc_text(self.nrc)
    }
}

pub fn nrc_text(nrc: u8) -> &'static str {
    match nrc {
        0x10 => "generalReject",
        0x11 => "serviceNotSupported",
        0x12 => "subFunctionNotSupported",
        0x13 => "incorrectMessageLengthOrInvalidFormat",
        0x14 => "responseTooLong",
        0x21 => "busyRepeatRequest",
        0x22 => "conditionsNotCorrect",
        0x24 => "requestSequenceError",
        0x25 => "noResponseFromSubnetComponent",
        0x26 => "failurePreventsExecutionOfRequestedAction",
        0x31 => "requestOutOfRange",
        0x33 => "securityAccessDenied",
        0x35 => "invalidKey",
        0x36 => "exceedNumberOfAttempts",
        0x37 => "requiredTimeDelayNotExpired",
        0x70 => "uploadDownloadNotAccepted",
        0x71 => "transferDataSuspended",
        0x72 => "generalProgrammingFailure",
        0x73 => "wrongBlockSequenceCounter",
        0x78 => "requestCorrectlyReceivedResponsePending",
        0x7E => "subFunctionNotSupportedInActiveSession",
        0x7F => "serviceNotSupportedInActiveSession",
        _ => "unknownNRC",
    }
}

/// Parse a UDS response: Ok(payload without positive SID) or Err(NRC).
/// Accepts both full positive (`SID+0x40 …`) and already-stripped payloads.
pub fn parse_response(expected_pos_sid: u8, resp: &[u8]) -> Result<Vec<u8>, NegativeResponse> {
    if resp.is_empty() {
        return Err(NegativeResponse {
            sid: expected_pos_sid.wrapping_sub(0x40),
            nrc: 0x10,
        });
    }
    if resp[0] == 0x7F {
        let sid = resp.get(1).copied().unwrap_or(0);
        let nrc = resp.get(2).copied().unwrap_or(0x10);
        return Err(NegativeResponse { sid, nrc });
    }
    if resp[0] == expected_pos_sid {
        return Ok(resp[1..].to_vec());
    }
    // Some transports return payload without SID echo
    Ok(resp.to_vec())
}

// ── Payload builders ────────────────────────────────────────────────────────

pub fn build_session_control(session: DiagnosticSession) -> Vec<u8> {
    vec![session as u8]
}

pub fn build_tester_present(suppress_response: bool) -> Vec<u8> {
    let sub = if suppress_response { 0x80 } else { 0x00 };
    vec![sub]
}

pub fn build_ecu_reset(reset_type: u8) -> Vec<u8> {
    vec![reset_type]
}

pub fn build_communication_control(control_type: u8, communication_type: u8) -> Vec<u8> {
    vec![control_type, communication_type]
}

pub fn build_control_dtc_setting(setting: u8) -> Vec<u8> {
    vec![setting]
}

pub fn build_read_did(did: u16) -> Vec<u8> {
    did.to_be_bytes().to_vec()
}

pub fn build_write_did(did: u16, data: &[u8]) -> Vec<u8> {
    let mut p = did.to_be_bytes().to_vec();
    p.extend_from_slice(data);
    p
}

pub fn build_read_memory(alfi: Alfi, address: u32, size: u32) -> Result<Vec<u8>, String> {
    let addr_len = alfi.address_bytes();
    let size_len = alfi.size_bytes();
    if addr_len == 0 || addr_len > 4 || size_len == 0 || size_len > 4 {
        return Err(format!("Invalid ALFI 0x{:02X}", alfi.0));
    }
    let mut p = Vec::with_capacity(1 + addr_len + size_len);
    p.push(alfi.0);
    let addr_be = address.to_be_bytes();
    p.extend_from_slice(&addr_be[4 - addr_len..]);
    let size_be = size.to_be_bytes();
    p.extend_from_slice(&size_be[4 - size_len..]);
    Ok(p)
}

pub fn build_read_memory_legacy42(address: u32, size: u16) -> Vec<u8> {
    let mut p = vec![0x42];
    p.extend_from_slice(&address.to_be_bytes());
    p.extend_from_slice(&size.to_be_bytes());
    p
}

pub fn build_security_request_seed(level: u8) -> Vec<u8> {
    vec![level]
}

pub fn build_security_send_key(level: u8, key: &[u8]) -> Vec<u8> {
    let mut p = vec![level.wrapping_add(1)];
    p.extend_from_slice(key);
    p
}

pub fn build_routine_control(sub: u8, routine_id: u16, option: &[u8]) -> Vec<u8> {
    let mut p = vec![sub];
    p.extend_from_slice(&routine_id.to_be_bytes());
    p.extend_from_slice(option);
    p
}

pub fn build_request_download(
    data_format: u8,
    alfi: Alfi,
    address: u32,
    size: u32,
) -> Result<Vec<u8>, String> {
    let addr_len = alfi.address_bytes();
    let size_len = alfi.size_bytes();
    if addr_len == 0 || addr_len > 4 || size_len == 0 || size_len > 4 {
        return Err(format!("Invalid ALFI 0x{:02X}", alfi.0));
    }
    let mut p = Vec::with_capacity(2 + addr_len + size_len);
    p.push(data_format);
    p.push(alfi.0);
    let addr_be = address.to_be_bytes();
    p.extend_from_slice(&addr_be[4 - addr_len..]);
    let size_be = size.to_be_bytes();
    p.extend_from_slice(&size_be[4 - size_len..]);
    Ok(p)
}

pub fn build_transfer_data(block_sequence: u8, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(1 + data.len());
    p.push(block_sequence);
    p.extend_from_slice(data);
    p
}

pub fn build_transfer_exit(params: &[u8]) -> Vec<u8> {
    params.to_vec()
}

pub fn build_clear_dtc(group: u32) -> Vec<u8> {
    vec![((group >> 16) & 0xFF) as u8, ((group >> 8) & 0xFF) as u8, (group & 0xFF) as u8]
}

pub fn build_read_dtc_by_status_mask(status_mask: u8) -> Vec<u8> {
    vec![0x02, status_mask]
}

pub fn build_read_dtc_number_by_status_mask(status_mask: u8) -> Vec<u8> {
    vec![0x01, status_mask]
}

// ── High-level helpers (ISO-TP via can.rs) ───────────────────────────────────

fn transact(
    port: &mut Box<dyn SerialPort + Send>,
    sid: u8,
    data: &[u8],
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    crate::can::uds_request(port, sid, data, use_elm)
}

fn transact_mf(
    port: &mut Box<dyn SerialPort + Send>,
    sid: u8,
    data: &[u8],
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    crate::can::uds_request_multiframe(port, sid, data, use_elm)
}

fn expect_positive(sid: u8, resp: &[u8]) -> Result<Vec<u8>, String> {
    let pos = sid.wrapping_add(0x40);
    match parse_response(pos, resp) {
        Ok(p) => Ok(p),
        Err(n) => Err(format!(
            "UDS NRC 0x{:02X} on SID 0x{:02X}: {}",
            n.nrc,
            n.sid,
            n.description()
        )),
    }
}

pub fn diagnostic_session_control(
    port: &mut Box<dyn SerialPort + Send>,
    session: DiagnosticSession,
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let resp = transact(port, SID_DSC, &build_session_control(session), use_elm)?;
    expect_positive(SID_DSC, &resp)
}

pub fn tester_present(
    port: &mut Box<dyn SerialPort + Send>,
    suppress_response: bool,
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact(port, SID_TP, &build_tester_present(suppress_response), use_elm)?;
    if suppress_response {
        return Ok(());
    }
    expect_positive(SID_TP, &resp).map(|_| ())
}

pub fn keep_alive_if_due(
    port: &mut Box<dyn SerialPort + Send>,
    last: &mut Instant,
    interval: Duration,
    use_elm: bool,
) {
    if last.elapsed() >= interval {
        let _ = tester_present(port, true, use_elm);
        *last = Instant::now();
    }
}

pub fn ecu_reset(
    port: &mut Box<dyn SerialPort + Send>,
    reset_type: u8,
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact(port, SID_ECU_RESET, &build_ecu_reset(reset_type), use_elm)?;
    expect_positive(SID_ECU_RESET, &resp).map(|_| ())
}

pub fn communication_control(
    port: &mut Box<dyn SerialPort + Send>,
    control_type: u8,
    communication_type: u8,
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact(
        port,
        SID_CC,
        &build_communication_control(control_type, communication_type),
        use_elm,
    )?;
    expect_positive(SID_CC, &resp).map(|_| ())
}

pub fn control_dtc_setting(
    port: &mut Box<dyn SerialPort + Send>,
    setting: u8,
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact(port, SID_CDTCS, &build_control_dtc_setting(setting), use_elm)?;
    expect_positive(SID_CDTCS, &resp).map(|_| ())
}

pub fn read_data_by_identifier(
    port: &mut Box<dyn SerialPort + Send>,
    did: u16,
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let resp = transact_mf(port, SID_RDBI, &build_read_did(did), use_elm)?;
    let payload = expect_positive(SID_RDBI, &resp)?;
    if payload.len() >= 2 {
        Ok(payload[2..].to_vec())
    } else {
        Ok(payload)
    }
}

pub fn read_memory_by_address(
    port: &mut Box<dyn SerialPort + Send>,
    alfi: Alfi,
    address: u32,
    size: u32,
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let data = build_read_memory(alfi, address, size)?;
    let resp = transact_mf(port, SID_RMBA, &data, use_elm)?;
    expect_positive(SID_RMBA, &resp)
}

pub fn read_memory_legacy42(
    port: &mut Box<dyn SerialPort + Send>,
    address: u32,
    size: u16,
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let resp = transact_mf(port, SID_RMBA, &build_read_memory_legacy42(address, size), use_elm)?;
    expect_positive(SID_RMBA, &resp)
}

pub fn routine_control(
    port: &mut Box<dyn SerialPort + Send>,
    sub: u8,
    routine_id: u16,
    option: &[u8],
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let resp = transact(
        port,
        SID_RC,
        &build_routine_control(sub, routine_id, option),
        use_elm,
    )?;
    expect_positive(SID_RC, &resp)
}

pub fn request_download(
    port: &mut Box<dyn SerialPort + Send>,
    alfi: Alfi,
    address: u32,
    size: u32,
    use_elm: bool,
) -> Result<usize, String> {
    let data = build_request_download(0x00, alfi, address, size)?;
    let resp = transact(port, SID_RD, &data, use_elm)?;
    let payload = expect_positive(SID_RD, &resp)?;
    if payload.is_empty() {
        return Ok(0x402);
    }
    let len_fi = payload[0];
    let n = (len_fi >> 4) as usize;
    if n == 0 || payload.len() < 1 + n {
        return Ok(0x402);
    }
    let mut max_block: u32 = 0;
    for &b in &payload[1..1 + n] {
        max_block = (max_block << 8) | (b as u32);
    }
    Ok(max_block.max(2) as usize)
}

pub fn transfer_data(
    port: &mut Box<dyn SerialPort + Send>,
    block_sequence: u8,
    data: &[u8],
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact_mf(port, SID_TD, &build_transfer_data(block_sequence, data), use_elm)?;
    expect_positive(SID_TD, &resp).map(|_| ())
}

pub fn transfer_exit(
    port: &mut Box<dyn SerialPort + Send>,
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact(port, SID_RTE, &[], use_elm)?;
    expect_positive(SID_RTE, &resp).map(|_| ())
}

pub fn download_image<F>(
    port: &mut Box<dyn SerialPort + Send>,
    alfi: Alfi,
    address: u32,
    image: &[u8],
    use_elm: bool,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(u32, u32),
{
    let max_block = request_download(port, alfi, address, image.len() as u32, use_elm)?;
    let chunk = max_block.saturating_sub(1).max(1).min(0x800);
    let mut seq: u8 = 1;
    let mut offset = 0usize;
    let total = image.len() as u32;
    let mut last_tp = Instant::now();
    while offset < image.len() {
        keep_alive_if_due(port, &mut last_tp, Duration::from_secs(2), use_elm);
        let end = (offset + chunk).min(image.len());
        transfer_data(port, seq, &image[offset..end], use_elm)?;
        offset = end;
        seq = seq.wrapping_add(1);
        if seq == 0 {
            seq = 1;
        }
        on_progress(offset as u32, total);
    }
    transfer_exit(port, use_elm)?;
    Ok(())
}

pub fn clear_diagnostic_information(
    port: &mut Box<dyn SerialPort + Send>,
    group: u32,
    use_elm: bool,
) -> Result<(), String> {
    let resp = transact(port, SID_CLEAR_DTC, &build_clear_dtc(group), use_elm)?;
    expect_positive(SID_CLEAR_DTC, &resp).map(|_| ())
}

pub fn read_dtc_by_status_mask(
    port: &mut Box<dyn SerialPort + Send>,
    status_mask: u8,
    use_elm: bool,
) -> Result<Vec<u8>, String> {
    let resp = transact_mf(
        port,
        SID_READ_DTC,
        &build_read_dtc_by_status_mask(status_mask),
        use_elm,
    )?;
    expect_positive(SID_READ_DTC, &resp)
}

pub fn prepare_programming_environment(
    port: &mut Box<dyn SerialPort + Send>,
    use_elm: bool,
    silence_bus: bool,
) -> Result<(), String> {
    match diagnostic_session_control(port, DiagnosticSession::Extended, use_elm) {
        Ok(_) => {}
        Err(_) => {
            diagnostic_session_control(port, DiagnosticSession::Programming, use_elm)?;
        }
    }
    let _ = control_dtc_setting(port, 0x02, use_elm);
    if silence_bus {
        let _ = communication_control(port, 0x03, 0x01, use_elm);
    }
    let _ = tester_present(port, true, use_elm);
    Ok(())
}

pub fn restore_default_environment(
    port: &mut Box<dyn SerialPort + Send>,
    use_elm: bool,
) -> Result<(), String> {
    let _ = communication_control(port, 0x00, 0x01, use_elm);
    let _ = control_dtc_setting(port, 0x01, use_elm);
    diagnostic_session_control(port, DiagnosticSession::Default, use_elm)?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SID constants ───────────────────────────────────────────────────────

    #[test]
    fn sid_constants_match_iso14229() {
        assert_eq!(SID_DSC, 0x10);
        assert_eq!(SID_ECU_RESET, 0x11);
        assert_eq!(SID_CLEAR_DTC, 0x14);
        assert_eq!(SID_READ_DTC, 0x19);
        assert_eq!(SID_RDBI, 0x22);
        assert_eq!(SID_RMBA, 0x23);
        assert_eq!(SID_SA, 0x27);
        assert_eq!(SID_CC, 0x28);
        assert_eq!(SID_WDBI, 0x2E);
        assert_eq!(SID_RC, 0x31);
        assert_eq!(SID_RD, 0x34);
        assert_eq!(SID_TD, 0x36);
        assert_eq!(SID_RTE, 0x37);
        assert_eq!(SID_TP, 0x3E);
        assert_eq!(SID_CDTCS, 0x85);
    }

    // ── DiagnosticSession ───────────────────────────────────────────────────

    #[test]
    fn session_enum_values() {
        assert_eq!(DiagnosticSession::Default as u8, 0x01);
        assert_eq!(DiagnosticSession::Programming as u8, 0x02);
        assert_eq!(DiagnosticSession::Extended as u8, 0x03);
        assert_eq!(DiagnosticSession::SafetySystem as u8, 0x04);
    }

    #[test]
    fn session_from_u8_roundtrip() {
        for v in [0x01u8, 0x02, 0x03, 0x04] {
            let s = DiagnosticSession::from_u8(v).unwrap();
            assert_eq!(s as u8, v);
        }
        assert_eq!(DiagnosticSession::from_u8(0x00), None);
        assert_eq!(DiagnosticSession::from_u8(0xFF), None);
    }

    #[test]
    fn build_session_control_payload() {
        assert_eq!(build_session_control(DiagnosticSession::Extended), vec![0x03]);
        assert_eq!(build_session_control(DiagnosticSession::Programming), vec![0x02]);
        assert_eq!(build_session_control(DiagnosticSession::Default), vec![0x01]);
    }

    // ── ALFI ────────────────────────────────────────────────────────────────

    #[test]
    fn alfi_lengths() {
        assert_eq!(Alfi::ADDR4_SIZE2.address_bytes(), 4);
        assert_eq!(Alfi::ADDR4_SIZE2.size_bytes(), 2);
        assert_eq!(Alfi::ADDR4_SIZE4.address_bytes(), 4);
        assert_eq!(Alfi::ADDR4_SIZE4.size_bytes(), 4);
        assert_eq!(Alfi::ADDR3_SIZE2.address_bytes(), 3);
        assert_eq!(Alfi::ADDR3_SIZE2.size_bytes(), 2);
        // PACKED_42 is non-ISO packing used by legacy flash path
        assert_eq!(Alfi::PACKED_42.address_bytes(), 2);
        assert_eq!(Alfi::PACKED_42.size_bytes(), 4);
    }

    #[test]
    fn alfi_invalid_rejected() {
        assert!(build_read_memory(Alfi(0x00), 0, 0).is_err());
        assert!(build_read_memory(Alfi(0x05), 0, 0).is_err()); // addr_len 5 > 4
        assert!(build_request_download(0x00, Alfi(0x50), 0, 0).is_err()); // size_len 5
    }

    #[test]
    fn build_rmba_24() {
        let p = build_read_memory(Alfi(0x24), 0x0008_0000, 0x400).unwrap();
        assert_eq!(p[0], 0x24);
        assert_eq!(&p[1..5], &[0x00, 0x08, 0x00, 0x00]);
        assert_eq!(&p[5..7], &[0x04, 0x00]);
        assert_eq!(p.len(), 7);
    }

    #[test]
    fn build_rmba_23_three_byte_addr() {
        let p = build_read_memory(Alfi(0x23), 0x00_ABCD, 0x10).unwrap();
        assert_eq!(p[0], 0x23);
        assert_eq!(&p[1..4], &[0x00, 0xAB, 0xCD]);
        assert_eq!(&p[4..6], &[0x00, 0x10]);
        assert_eq!(p.len(), 6);
    }

    #[test]
    fn build_rmba_44_large() {
        let p = build_read_memory(Alfi(0x44), 0x0000_0000, 0x0020_0000).unwrap();
        assert_eq!(p[0], 0x44);
        assert_eq!(&p[1..5], &[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(&p[5..9], &[0x00, 0x20, 0x00, 0x00]);
        assert_eq!(p.len(), 9);
    }

    #[test]
    fn build_rmba_legacy42() {
        let p = build_read_memory_legacy42(0x0008_0000, 0x400);
        assert_eq!(p[0], 0x42);
        assert_eq!(&p[1..5], &[0x00, 0x08, 0x00, 0x00]);
        assert_eq!(&p[5..7], &[0x04, 0x00]);
        assert_eq!(p.len(), 7);
    }

    // ── parse_response / NRC ────────────────────────────────────────────────

    #[test]
    fn parse_positive_strips_sid() {
        let r = parse_response(0x50, &[0x50, 0x03]).unwrap();
        assert_eq!(r, vec![0x03]);
    }

    #[test]
    fn parse_positive_empty_payload() {
        let r = parse_response(0x7E, &[0x7E]).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn parse_stripped_payload_passthrough() {
        // Transport already removed SID
        let r = parse_response(0x63, &[0xDE, 0xAD]).unwrap();
        assert_eq!(r, vec![0xDE, 0xAD]);
    }

    #[test]
    fn parse_empty_is_nrc() {
        let e = parse_response(0x50, &[]).unwrap_err();
        assert_eq!(e.nrc, 0x10);
        assert_eq!(e.sid, 0x10); // 0x50 - 0x40
    }

    #[test]
    fn parse_nrc_conditions_not_correct() {
        let e = parse_response(0x50, &[0x7F, 0x10, 0x22]).unwrap_err();
        assert_eq!(e.sid, 0x10);
        assert_eq!(e.nrc, 0x22);
        assert!(e.description().contains("conditions"));
    }

    #[test]
    fn parse_nrc_invalid_key() {
        let e = parse_response(0x67, &[0x7F, 0x27, 0x35]).unwrap_err();
        assert_eq!(e.sid, 0x27);
        assert_eq!(e.nrc, 0x35);
        assert_eq!(e.description(), "invalidKey");
    }

    #[test]
    fn parse_nrc_short_frame_defaults() {
        let e = parse_response(0x50, &[0x7F]).unwrap_err();
        assert_eq!(e.sid, 0);
        assert_eq!(e.nrc, 0x10);
    }

    #[test]
    fn nrc_text_catalogue() {
        assert_eq!(nrc_text(0x11), "serviceNotSupported");
        assert_eq!(nrc_text(0x33), "securityAccessDenied");
        assert_eq!(nrc_text(0x36), "exceedNumberOfAttempts");
        assert_eq!(nrc_text(0x37), "requiredTimeDelayNotExpired");
        assert_eq!(nrc_text(0x70), "uploadDownloadNotAccepted");
        assert_eq!(nrc_text(0x73), "wrongBlockSequenceCounter");
        assert_eq!(nrc_text(0x78), "requestCorrectlyReceivedResponsePending");
        assert_eq!(nrc_text(0x7F), "serviceNotSupportedInActiveSession");
        assert_eq!(nrc_text(0xAB), "unknownNRC");
    }

    #[test]
    fn expect_positive_formats_error() {
        let err = expect_positive(0x10, &[0x7F, 0x10, 0x7E]).unwrap_err();
        assert!(err.contains("0x7E"));
        assert!(err.contains("0x10"));
        assert!(err.contains("subFunctionNotSupportedInActiveSession"));
    }

    #[test]
    fn expect_positive_ok() {
        let p = expect_positive(0x3E, &[0x7E, 0x00]).unwrap();
        assert_eq!(p, vec![0x00]);
    }

    // ── Session / keep-alive / reset builders ───────────────────────────────

    #[test]
    fn tester_present_suppress_bit() {
        assert_eq!(build_tester_present(false), vec![0x00]);
        assert_eq!(build_tester_present(true), vec![0x80]);
    }

    #[test]
    fn ecu_reset_types() {
        assert_eq!(build_ecu_reset(0x01), vec![0x01]); // hard
        assert_eq!(build_ecu_reset(0x02), vec![0x02]); // keyOffOn
        assert_eq!(build_ecu_reset(0x03), vec![0x03]); // soft
    }

    #[test]
    fn communication_control_payload() {
        assert_eq!(build_communication_control(0x03, 0x01), vec![0x03, 0x01]);
        assert_eq!(build_communication_control(0x00, 0x01), vec![0x00, 0x01]);
    }

    #[test]
    fn control_dtc_setting_on_off() {
        assert_eq!(build_control_dtc_setting(0x01), vec![0x01]);
        assert_eq!(build_control_dtc_setting(0x02), vec![0x02]);
    }

    // ── DID / Security / Routine ────────────────────────────────────────────

    #[test]
    fn build_read_did_be() {
        assert_eq!(build_read_did(0xF190), vec![0xF1, 0x90]); // VIN
        assert_eq!(build_read_did(0xF187), vec![0xF1, 0x87]);
    }

    #[test]
    fn build_write_did_appends_data() {
        let p = build_write_did(0xF190, b"TEST");
        assert_eq!(&p[..2], &[0xF1, 0x90]);
        assert_eq!(&p[2..], b"TEST");
    }

    #[test]
    fn security_seed_and_key_levels() {
        assert_eq!(build_security_request_seed(0x01), vec![0x01]);
        assert_eq!(build_security_request_seed(0x03), vec![0x03]);
        let key = build_security_send_key(0x01, &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(key, vec![0x02, 0xAA, 0xBB, 0xCC, 0xDD]);
        let key2 = build_security_send_key(0x03, &[0x11, 0x22]);
        assert_eq!(key2, vec![0x04, 0x11, 0x22]);
    }

    #[test]
    fn routine_control_start_stop_results() {
        let start = build_routine_control(0x01, 0xFF00, &[0x01]);
        assert_eq!(start, vec![0x01, 0xFF, 0x00, 0x01]);
        let stop = build_routine_control(0x02, 0xFF00, &[]);
        assert_eq!(stop, vec![0x02, 0xFF, 0x00]);
        let results = build_routine_control(0x03, 0x0202, &[]);
        assert_eq!(results, vec![0x03, 0x02, 0x02]);
    }

    // ── Download / transfer ───────────────────────────────────────────────────

    #[test]
    fn build_request_download_44() {
        let p = build_request_download(0x00, Alfi(0x44), 0x80000, 0x20000).unwrap();
        assert_eq!(p[0], 0x00); // dataFormatIdentifier
        assert_eq!(p[1], 0x44);
        assert_eq!(&p[2..6], &[0x00, 0x08, 0x00, 0x00]);
        assert_eq!(&p[6..10], &[0x00, 0x02, 0x00, 0x00]);
        assert_eq!(p.len(), 10);
    }

    #[test]
    fn build_request_download_24() {
        let p = build_request_download(0x00, Alfi::ADDR4_SIZE2, 0x0008_0000, 0x1000).unwrap();
        assert_eq!(p[0], 0x00);
        assert_eq!(p[1], 0x24);
        assert_eq!(p.len(), 2 + 4 + 2);
    }

    #[test]
    fn transfer_data_sequence_counter() {
        assert_eq!(build_transfer_data(1, &[0xAA, 0xBB]), vec![0x01, 0xAA, 0xBB]);
        assert_eq!(build_transfer_data(0xFF, &[]), vec![0xFF]);
        assert_eq!(build_transfer_data(0, &[0x00]), vec![0x00, 0x00]);
    }

    #[test]
    fn transfer_exit_empty_and_params() {
        assert!(build_transfer_exit(&[]).is_empty());
        assert_eq!(build_transfer_exit(&[0x01, 0x02]), vec![0x01, 0x02]);
    }

    /// Simulate parsing maxNumberOfBlockLength from a positive 0x74 response payload.
    #[test]
    fn parse_max_block_length_from_rd_payload() {
        // lengthFormatIdentifier = 0x20 → 2 bytes length; max block = 0x0402
        let payload = [0x20u8, 0x04, 0x02];
        let n = (payload[0] >> 4) as usize;
        assert_eq!(n, 2);
        let mut max_block: u32 = 0;
        for &b in &payload[1..1 + n] {
            max_block = (max_block << 8) | (b as u32);
        }
        assert_eq!(max_block, 0x0402);
        // chunk size for TransferData data bytes
        let chunk = (max_block as usize).saturating_sub(1).max(1).min(0x800);
        assert_eq!(chunk, 0x401);
    }

    #[test]
    fn parse_max_block_single_byte() {
        let payload = [0x10u8, 0x80]; // 1-byte max = 128
        let n = (payload[0] >> 4) as usize;
        let mut max_block: u32 = 0;
        for &b in &payload[1..1 + n] {
            max_block = (max_block << 8) | (b as u32);
        }
        assert_eq!(max_block, 0x80);
    }

    // ── DTC builders ────────────────────────────────────────────────────────

    #[test]
    fn clear_dtc_all_group() {
        assert_eq!(build_clear_dtc(0x00FF_FFFF), vec![0xFF, 0xFF, 0xFF]);
        assert_eq!(build_clear_dtc(0), vec![0x00, 0x00, 0x00]);
        assert_eq!(build_clear_dtc(0x00_1234), vec![0x00, 0x12, 0x34]);
    }

    #[test]
    fn read_dtc_status_mask_subfunctions() {
        assert_eq!(build_read_dtc_by_status_mask(0xFF), vec![0x02, 0xFF]);
        assert_eq!(build_read_dtc_number_by_status_mask(0x09), vec![0x01, 0x09]);
        assert_eq!(build_read_dtc_by_status_mask(0x00), vec![0x02, 0x00]);
    }

    // ── Sequence counter wrap behaviour (documented in download_image) ──────

    #[test]
    fn block_sequence_skips_zero_after_wrap() {
        let mut seq: u8 = 0xFF;
        seq = seq.wrapping_add(1);
        if seq == 0 {
            seq = 1;
        }
        assert_eq!(seq, 1);

        seq = 1;
        for _ in 0..254 {
            seq = seq.wrapping_add(1);
            if seq == 0 {
                seq = 1;
            }
        }
        assert_eq!(seq, 0xFF);
        seq = seq.wrapping_add(1);
        if seq == 0 {
            seq = 1;
        }
        assert_eq!(seq, 1);
    }

    // ── Positive SID arithmetic ─────────────────────────────────────────────

    #[test]
    fn positive_response_sid_offset() {
        assert_eq!(SID_DSC.wrapping_add(0x40), 0x50);
        assert_eq!(SID_RMBA.wrapping_add(0x40), 0x63);
        assert_eq!(SID_RD.wrapping_add(0x40), 0x74);
        assert_eq!(SID_TD.wrapping_add(0x40), 0x76);
        assert_eq!(SID_RTE.wrapping_add(0x40), 0x77);
        assert_eq!(SID_TP.wrapping_add(0x40), 0x7E);
        assert_eq!(SID_SA.wrapping_add(0x40), 0x67);
    }
}
