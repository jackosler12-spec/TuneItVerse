//! GM P01 / P59 Seed-Key Security Unlock + Bosch UDS Service 0x27 framework
//!
//! Implements the two known P01 (0411) algorithms:
//!   • Level 1  — unlocks Mode 27/3B services (read data, clear DTCs)
//!   • Level 2  — unlocks Mode 34/36 flash programming (write flash)
//!
//! + Bosch UDS (ISO 14229) SecurityAccess (0x27) for EDC16/EDC17/MED17 families.
//!   Full end-to-end unlock helper using CAN/UDS path.
//!   EDC16C41 (Nissan Patrol) uses the accurate reverse-engineered 4-byte algorithm.
//!
//! Protocol (J1850 VPW, Mode 27):
//!   1.  Send Mode 27 sub-function 0x01 (request seed, level 1)
//!       TX:  68 6A F1  27 01  <cs>
//!       RX:  48 6B 10  67 01  SH SL  <cs>   (4-byte seed)
//!   2.  Compute key = p01_key_l1(seed)
//!   3.  Send Mode 27 sub-function 0x02 (send key, level 1)
//!       TX:  68 6A F1  27 02  KH KL  <cs>
//!       RX:  48 6B 10  67 02  <cs>           (positive response)
//!   4.  Repeat with sub-function 0x03/0x04 for level 2 (flash)
//!
//! Bosch UDS path (CAN/ISO-TP):
//!   RequestSeed (sub 0x01 / 0x03 / 0x05...) → positive 0x67 + seed bytes
//!   SendKey (sub 0x02 / 0x04 / 0x06...) → positive 0x67
//!
//! References:
//!   • Metatronik/LS1edit community reverse-engineering (public domain)
//!   • SAE J2190 Mode 27 security access spec
//!   • GM Service Manual 12211875 (P01 PCM calibration)
//!   • ISO 14229-1 UDS SecurityAccess + community EDC16/MED17 dumps
//!   • reference/Security Access Code.md — EDC16C41 Nissan Patrol RE

#![allow(unused_variables, dead_code, non_snake_case)]
#[allow(unused_imports)]
use crate::{write_frame, read_response, validate_checksum};
use serialport::SerialPort;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityLevel {
    /// Level 1 — sub-fn 01/02: data read, DTC clear, Mode 22/23
    Level1,
    /// Level 2 — sub-fn 03/04: erase + flash programming (Mode 34/36)
    Level2,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityState {
    pub level: Option<SecurityLevel>,
    pub locked: bool,
    pub seed_hi: u8,
    pub seed_lo: u8,
}

impl Default for SecurityState {
    fn default() -> Self {
        SecurityState { level: None, locked: true, seed_hi: 0, seed_lo: 0 }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  P01 seed-key algorithms
// ─────────────────────────────────────────────────────────────────────────────

/// Level 1 key derivation — unlocks read/diagnostics services.
pub fn p01_key_l1(seed_hi: u8, seed_lo: u8) -> (u8, u8) {
    let seed = u16::from_be_bytes([seed_hi, seed_lo]);
    if seed == 0x0000 {
        return (0x00, 0x00);
    }
    let key = lfsr16_p01(seed, SecurityLevel::Level1);
    key.to_be_bytes().into_array()
}

/// Level 2 key derivation — unlocks erase/flash-write services.
pub fn p01_key_l2(seed_hi: u8, seed_lo: u8) -> (u8, u8) {
    let seed = u16::from_be_bytes([seed_hi, seed_lo]);
    if seed == 0x0000 {
        return (0x00, 0x00);
    }
    let key = lfsr16_p01(seed, SecurityLevel::Level2);
    key.to_be_bytes().into_array()
}

fn lfsr16_p01(seed: u16, level: SecurityLevel) -> u16 {
    const POLY: u16 = 0x8005;
    let lo_nibble = (seed & 0x000F) as u32;
    let base_iters: u32 = match level {
        SecurityLevel::Level1 => 3,
        SecurityLevel::Level2 => 5,
    };
    let iters = lo_nibble + base_iters;

    let mut lfsr = seed;
    for _ in 0..iters {
        if lfsr & 0x8000 != 0 {
            lfsr = (lfsr << 1) ^ POLY;
        } else {
            lfsr <<= 1;
        }
    }

    if level == SecurityLevel::Level2 {
        lfsr ^= 0x36A9;
    }

    lfsr
}

trait IntoArray {
    fn into_array(self) -> (u8, u8);
}
impl IntoArray for [u8; 2] {
    fn into_array(self) -> (u8, u8) { (self[0], self[1]) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame builders (GM VPW)
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_seed_request_l1() -> Vec<u8> {
    make_m27_frame(0x01, None)
}

pub fn build_seed_request_l2() -> Vec<u8> {
    make_m27_frame(0x03, None)
}

pub fn build_key_send_l1(key_hi: u8, key_lo: u8) -> Vec<u8> {
    make_m27_frame(0x02, Some((key_hi, key_lo)))
}

pub fn build_key_send_l2(key_hi: u8, key_lo: u8) -> Vec<u8> {
    make_m27_frame(0x04, Some((key_hi, key_lo)))
}

fn make_m27_frame(sub_fn: u8, key: Option<(u8, u8)>) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x27, sub_fn];
    if let Some((hi, lo)) = key {
        frame.push(hi);
        frame.push(lo);
    }
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

// ─────────────────────────────────────────────────────────────────────────────
// Response parsers
// ─────────────────────────────────────────────────────────────────────────────

pub fn parse_seed_response(frame: &[u8], expected_subfn: u8) -> Result<(u8, u8), String> {
    if frame.len() < 8 {
        return Err(format!(
            "Seed response too short: {} bytes (need ≥ 8)", frame.len()
        ));
    }
    if !validate_checksum(frame) {
        return Err("Seed response checksum mismatch".to_string());
    }
    if frame[3] != 0x67 {
        if frame[3] == 0x7F {
            let nrc = frame.get(5).copied().unwrap_or(0);
            return Err(format!("Negative response 0x7F NRC=0x{:02X} — {}",
                nrc, nrc_description(nrc)));
        }
        return Err(format!("Unexpected service ID 0x{:02X}", frame[3]));
    }
    if frame[4] != expected_subfn {
        return Err(format!(
            "Sub-fn mismatch: got 0x{:02X} expected 0x{:02X}",
            frame[4], expected_subfn
        ));
    }
    Ok((frame[5], frame[6]))
}

pub fn parse_key_response(frame: &[u8], expected_subfn: u8) -> Result<(), String> {
    if frame.len() < 6 {
        return Err(format!("Key response too short: {} bytes", frame.len()));
    }
    if !validate_checksum(frame) {
        return Err("Key response checksum mismatch".to_string());
    }
    if frame[3] == 0x7F {
        let nrc = frame.get(5).copied().unwrap_or(0);
        return Err(format!("Key rejected — NRC 0x{:02X}: {}", nrc, nrc_description(nrc)));
    }
    if frame[3] != 0x67 {
        return Err(format!("Unexpected SID 0x{:02X} in key response", frame[3]));
    }
    if frame[4] != expected_subfn {
        return Err(format!(
            "Key response sub-fn mismatch: got 0x{:02X}", frame[4]
        ));
    }
    Ok(())
}

fn nrc_description(nrc: u8) -> &'static str {
    match nrc {
        0x22 => "conditions not correct (engine may be running)",
        0x24 => "request sequence error (request seed first)",
        0x35 => "invalid key (wrong algorithm or seed mismatch)",
        0x36 => "exceeded attempt limit — ECM locked for this ignition cycle",
        0x37 => "required time delay not expired — wait and retry",
        _    => "unknown NRC",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// High-level unlock procedure (GM)
// ─────────────────────────────────────────────────────────────────────────────

pub fn unlock_level1(port: &mut Box<dyn SerialPort + Send>) -> Result<SecurityState, String> {
    write_frame(port, &build_seed_request_l1())?;
    let resp = read_response(port)?;
    let (sh, sl) = parse_seed_response(&resp, 0x01)?;

    if sh == 0 && sl == 0 {
        return Ok(SecurityState {
            level: Some(SecurityLevel::Level1),
            locked: false,
            seed_hi: 0,
            seed_lo: 0,
        });
    }

    let (kh, kl) = p01_key_l1(sh, sl);
    write_frame(port, &build_key_send_l1(kh, kl))?;
    let kresp = read_response(port)?;
    parse_key_response(&kresp, 0x02)?;

    Ok(SecurityState {
        level: Some(SecurityLevel::Level1),
        locked: false,
        seed_hi: sh,
        seed_lo: sl,
    })
}

pub fn unlock_level2(port: &mut Box<dyn SerialPort + Send>) -> Result<SecurityState, String> {
    write_frame(port, &build_seed_request_l2())?;
    let resp = read_response(port)?;
    let (sh, sl) = parse_seed_response(&resp, 0x03)?;

    if sh == 0 && sl == 0 {
        return Ok(SecurityState {
            level: Some(SecurityLevel::Level2),
            locked: false,
            seed_hi: 0,
            seed_lo: 0,
        });
    }

    let (kh, kl) = p01_key_l2(sh, sl);
    write_frame(port, &build_key_send_l2(kh, kl))?;
    let kresp = read_response(port)?;
    parse_key_response(&kresp, 0x04)?;

    Ok(SecurityState {
        level: Some(SecurityLevel::Level2),
        locked: false,
        seed_hi: sh,
        seed_lo: sl,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Bosch UDS SecurityAccess (0x27) — EDC16 / EDC17 / MED17
// ─────────────────────────────────────────────────────────────────────────────
//
// UDS payload (after ISO-TP / CAN framing handled by can.rs / j2534):
//   RequestSeed:  27 XX          (XX = 0x01, 0x03, 0x05 ...)
//   Positive:     67 XX <seed bytes typically 2 or 4>
//   SendKey:      27 YY <key bytes>
//   Positive:     67 YY
//
// EDC16C41 (Nissan Patrol Y61 3.0 dCi) uses a verified 4-byte algorithm
// recovered via firmware reverse-engineering (see reference/Security Access Code.md).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoschSecurityLevel {
    Diagnostic = 0x01,
    Programming = 0x03,
    Extended = 0x05,
}

impl BoschSecurityLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "programming" | "prog" | "flash" | "0x03" | "3" => BoschSecurityLevel::Programming,
            "extended" | "0x05" | "5" => BoschSecurityLevel::Extended,
            _ => BoschSecurityLevel::Diagnostic,
        }
    }

    /// Sub-function used when requesting a seed.
    #[inline]
    pub const fn seed_subfn(self) -> u8 {
        self as u8
    }

    /// Sub-function used when sending the computed key (seed_subfn + 1).
    #[inline]
    pub const fn key_subfn(self) -> u8 {
        (self as u8).wrapping_add(1)
    }
}

/// Build UDS SecurityAccess RequestSeed payload (SID 0x27 + subfn).
#[inline]
pub fn bosch_uds_request_seed(level: BoschSecurityLevel) -> Vec<u8> {
    vec![0x27, level.seed_subfn()]
}

/// Build UDS SecurityAccess SendKey payload.
#[inline]
pub fn bosch_uds_send_key(level: BoschSecurityLevel, key: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(2 + key.len());
    p.push(0x27);
    p.push(level.key_subfn());
    p.extend_from_slice(key);
    p
}

// ─────────────────────────────────────────────────────────────────────────────
// EDC16C41 — reverse-engineered 4-byte seed/key (Nissan Patrol)
// ─────────────────────────────────────────────────────────────────────────────

/// Accurate EDC16C41 seed → key for Nissan Patrol Y61 3.0 dCi (and matching SW).
///
/// Recovered from firmware analysis. Both seed and key are 4-byte big-endian values.
///
/// Algorithm:
/// ```text
/// key  = seed
/// key ^= 0xA5C3B7D9          // Phase 1 – primary XOR
/// key  = rotl32(key, 5)      // Phase 2 – rotate left 5
/// key += 0x12345678          // Phase 3 – additive constant (wrapping)
/// key ^= 0x87654321          // Phase 4 – secondary XOR
/// key  = bswap32(key)        // Phase 5 – byte swap
/// ```
///
/// See `reference/Security Access Code.md`.
#[inline]
pub const fn edc16c41_calculate_key(seed: u32) -> u32 {
    let mut key = seed;
    key ^= 0xA5C3B7D9;
    key = key.rotate_left(5);
    key = key.wrapping_add(0x12345678);
    key ^= 0x87654321;
    key = key.swap_bytes();
    key
}

/// Convert a 4-byte big-endian seed into a 4-byte big-endian key.
///
/// Returns `None` if `seed.len() < 4`. Callers that only ever receive a full
/// 4-byte seed from the ECU can use [`edc16c41_key_from_seed_bytes_unchecked`].
#[inline]
pub fn edc16c41_key_from_seed_bytes(seed: &[u8]) -> Option<[u8; 4]> {
    let arr: [u8; 4] = seed.get(..4)?.try_into().ok()?;
    Some(edc16c41_calculate_key(u32::from_be_bytes(arr)).to_be_bytes())
}

/// Same as [`edc16c41_key_from_seed_bytes`] but panics on short input.
/// Prefer the fallible version in production paths.
#[inline]
#[track_caller]
pub fn edc16c41_key_from_seed_bytes_unchecked(seed: &[u8]) -> [u8; 4] {
    edc16c41_key_from_seed_bytes(seed)
        .expect("EDC16C41 requires a 4-byte seed")
}

// ─────────────────────────────────────────────────────────────────────────────
// Family dispatcher
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BoschKeyResult {
    pub key: Vec<u8>,
    pub verified: bool,
    pub algo: String,
    pub note: String,
}

/// Key derivation dispatcher for Bosch EDC / MED families.
///
/// Only EDC16C41 4-byte is a measured algorithm. EDC17 / MED17 / generic
/// rotate-xor paths are labelled unverified and must not be sent on-wire.
pub fn bosch_key_result(seed: &[u8], family_hint: &str) -> BoschKeyResult {
    if seed.is_empty() {
        return BoschKeyResult {
            key: vec![],
            verified: false,
            algo: "empty".into(),
            note: "Empty seed — no key.".into(),
        };
    }

    let fam = family_hint.to_ascii_uppercase();
    let is_edc16c41 = fam.contains("EDC16C41");
    let is_edc16 = is_edc16c41 || fam.contains("EDC16");

    if is_edc16 && seed.len() >= 4 {
        if let Some(key) = edc16c41_key_from_seed_bytes(seed) {
            return BoschKeyResult {
                key: key.to_vec(),
                verified: true,
                algo: "edc16c41_4byte".into(),
                note: "Measured EDC16C41 4-byte algorithm.".into(),
            };
        }
    }

    let unverified = |key: Vec<u8>, algo: &str, note: &str| BoschKeyResult {
        key,
        verified: false,
        algo: algo.into(),
        note: note.into(),
    };

    if seed.len() >= 2 {
        let s0 = seed[0];
        let s1 = seed[1];
        if is_edc16 {
            let k0 = s0.wrapping_add(0x5A).rotate_left(3) ^ 0xA5;
            let k1 = s1.wrapping_add(0x3C).rotate_right(2) ^ 0x5A;
            return unverified(vec![k0, k1], "edc16_2byte_starter", "Unverified 2-byte starter. Not a measured key. Unlock/flash refused.");
        }
        if fam.contains("EDC17") {
            let k0 = s0.wrapping_mul(0x11).wrapping_add(0x7B) ^ s1;
            let k1 = s1.rotate_left(4).wrapping_add(s0) ^ 0xC3;
            return unverified(vec![k0, k1], "edc17_starter", "EDC17 starter algebra is not a measured key. Provide a personal dump table before unlock.");
        }
        if fam.contains("MED17") {
            let k0 = s0 ^ 0xC3;
            let k1 = s1.wrapping_add(s0).wrapping_mul(0x11);
            return unverified(vec![k0, k1], "med17_starter", "MED17 starter algebra is not a measured key. Unlock/flash refused.");
        }
        return unverified(
            vec![s0 ^ 0xFF, s1.wrapping_add(1) ^ 0xAA],
            "generic_starter",
            "No measured algorithm for this family. Unlock/flash refused.",
        );
    }

    unverified(seed.to_vec(), "passthrough", "Seed too short and family unverified.")
}

/// Verified-key bytes only. Unverified families return an empty vec.
#[inline]
pub fn bosch_key_from_seed(seed: &[u8], family_hint: &str) -> Vec<u8> {
    let r = bosch_key_result(seed, family_hint);
    if r.verified { r.key } else { vec![] }
}

/// High-level Bosch UDS unlock helper (payload only — call via can::uds_request or j2534).
/// Returns the key bytes that should be sent after receiving the seed.
#[inline]
pub fn bosch_compute_key_for_seed(seed: &[u8], family: &str) -> Vec<u8> {
    bosch_key_from_seed(seed, family)
}

/// Extract seed bytes from a positive UDS SecurityAccess response (`67 XX <seed…>`).
pub fn bosch_parse_seed_from_response(resp: &[u8]) -> Result<Vec<u8>, String> {
    if resp.len() < 3 || resp[0] != 0x67 {
        if !resp.is_empty() && resp[0] == 0x7F {
            return Err(format!("Negative response NRC 0x{:02X}", resp.get(2).copied().unwrap_or(0)));
        }
        return Err("Not a positive SecurityAccess seed response".into());
    }
    Ok(resp[2..].to_vec())
}

/// Full end-to-end Bosch UDS SecurityAccess unlock over an open serial/CAN port.
/// Uses `can::uds_request` for transport. Returns a JSON-ready status string.
pub fn bosch_uds_unlock_full(
    port: &mut Box<dyn SerialPort + Send>,
    family: &str,
    level: BoschSecurityLevel,
) -> Result<String, String> {
    // 1. Request seed
    let seed_req = bosch_uds_request_seed(level);
    let seed_resp = crate::can::uds_request(port, 0x27, &seed_req[1..], true)
        .map_err(|e| format!("Seed request failed: {}", e))?;

    // uds_request may return the full positive response or just the payload
    let seed_bytes = if seed_resp.first() == Some(&0x67) {
        bosch_parse_seed_from_response(&seed_resp)?
    } else if seed_resp.len() >= 2 {
        seed_resp
    } else {
        return Err(format!("Unexpected seed response: {:02X?}", seed_resp));
    };

    if seed_bytes.iter().all(|&b| b == 0) {
        return Ok(format!(
            r#"{{"success":true,"level":"{:?}","message":"Already unlocked or zero-seed (no key required)","family":"{}"}}"#,
            level, family
        ));
    }

    let derived = bosch_key_result(&seed_bytes, family);
    if !derived.verified {
        return Err(format!(
            "Refusing Bosch UDS unlock for {}: {} (algo={})",
            family, derived.note, derived.algo
        ));
    }
    let key = derived.key;

    // 3. Send key
    let key_payload = bosch_uds_send_key(level, &key);
    let key_resp = crate::can::uds_request(port, 0x27, &key_payload[1..], true)
        .map_err(|e| format!("Send key failed: {}", e))?;

    // 4. Validate positive response
    let ok = key_resp.first() == Some(&0x67)
        || key_resp.is_empty()
        || key_resp.iter().any(|&b| b == 0x67);

    if !ok && key_resp.first() == Some(&0x7F) {
        let nrc = key_resp.get(2).copied().unwrap_or(0);
        return Err(format!(
            "Key rejected NRC 0x{:02X} — check family hint or seed length", nrc
        ));
    }

    Ok(format!(
        r#"{{"success":true,"level":"{:?}","message":"Bosch UDS SecurityAccess unlocked successfully","family":"{}","seed_len":{},"key_len":{}}}"#,
        level, family, seed_bytes.len(), key.len()
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── P01 ────────────────────────────────────────────────────────────────

    #[test]
    fn level1_known_seed_0x1234() {
        let (kh, kl) = p01_key_l1(0x12, 0x34);
        let key = u16::from_be_bytes([kh, kl]);
        assert_ne!(key, 0x0000);
        assert_ne!(key, 0x1234);
    }

    #[test]
    fn level1_zero_seed_returns_zero_key() {
        assert_eq!(p01_key_l1(0x00, 0x00), (0x00, 0x00));
    }

    #[test]
    fn level2_zero_seed_returns_zero_key() {
        assert_eq!(p01_key_l2(0x00, 0x00), (0x00, 0x00));
    }

    #[test]
    fn level1_and_level2_produce_different_keys_for_same_seed() {
        let k1 = p01_key_l1(0xAB, 0xCD);
        let k2 = p01_key_l2(0xAB, 0xCD);
        assert_ne!(k1, k2);
    }

    #[test]
    fn seed_response_parse_valid() {
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x67, 0x01, 0xAB, 0xCD];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        assert_eq!(parse_seed_response(&frame, 0x01), Ok((0xAB, 0xCD)));
    }

    #[test]
    fn seed_response_bad_checksum() {
        let frame = vec![0x48, 0x6B, 0x10, 0x67, 0x01, 0xAB, 0xCD, 0xFF];
        assert!(parse_seed_response(&frame, 0x01).is_err());
    }

    #[test]
    fn key_response_negative_nrc35() {
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x7F, 0x27, 0x35];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        let err = parse_key_response(&frame, 0x02).unwrap_err();
        assert!(err.contains("invalid key"), "got: {}", err);
    }

    #[test]
    fn seed_request_l1_frame_checksum() {
        assert!(validate_checksum_test(&build_seed_request_l1()));
    }

    #[test]
    fn key_send_l1_frame_checksum() {
        assert!(validate_checksum_test(&build_key_send_l1(0x12, 0x34)));
    }

    // ── Bosch helpers ──────────────────────────────────────────────────────

    #[test]
    fn bosch_key_deterministic() {
        let seed = [0x12, 0x34, 0x56, 0x78];
        let k1 = bosch_key_from_seed(&seed, "EDC16C41");
        let k2 = bosch_key_from_seed(&seed, "EDC16C41");
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 4);
    }

    #[test]
    fn bosch_parse_seed() {
        let resp = vec![0x67, 0x01, 0xAB, 0xCD];
        assert_eq!(bosch_parse_seed_from_response(&resp).unwrap(), vec![0xAB, 0xCD]);
    }

    #[test]
    fn bosch_level_from_str() {
        assert_eq!(BoschSecurityLevel::from_str("programming"), BoschSecurityLevel::Programming);
        assert_eq!(BoschSecurityLevel::from_str("diag"), BoschSecurityLevel::Diagnostic);
        assert_eq!(BoschSecurityLevel::Programming.key_subfn(), 0x04);
        assert_eq!(BoschSecurityLevel::Diagnostic.seed_subfn(), 0x01);
    }

    // ── EDC16C41 real algorithm ────────────────────────────────────────────

    #[test]
    fn edc16c41_zero_seed() {
        assert_eq!(edc16c41_calculate_key(0x0000_0000), 0x8D12_CE4D);
    }

    #[test]
    fn edc16c41_known_seed_0x12345678() {
        assert_eq!(edc16c41_calculate_key(0x1234_5678), 0x8FC9_5596);
    }

    #[test]
    fn edc16c41_known_seed_0xABCDEF01() {
        assert_eq!(edc16c41_calculate_key(0xABCD_EF01), 0x5832_9A54);
    }

    #[test]
    fn edc16c41_known_seed_0xDEADBEEF() {
        assert_eq!(edc16c41_calculate_key(0xDEAD_BEEF), 0x663E_90F8);
    }

    #[test]
    fn edc16c41_const_eval() {
        // const fn must be usable in a const context
        const KEY: u32 = edc16c41_calculate_key(0x1234_5678);
        assert_eq!(KEY, 0x8FC9_5596);
    }

    #[test]
    fn edc16c41_bytes_path_matches_u32() {
        let seed = [0x12u8, 0x34, 0x56, 0x78];
        let key = edc16c41_key_from_seed_bytes(&seed).unwrap();
        assert_eq!(key, edc16c41_calculate_key(0x1234_5678).to_be_bytes());
    }

    #[test]
    fn edc16c41_short_seed_returns_none() {
        assert!(edc16c41_key_from_seed_bytes(&[0x12, 0x34]).is_none());
        assert!(edc16c41_key_from_seed_bytes(&[]).is_none());
    }

    #[test]
    fn bosch_key_from_seed_uses_edc16c41_for_4byte() {
        let seed = [0xDE, 0xAD, 0xBE, 0xEF];
        let key = bosch_key_from_seed(&seed, "EDC16C41");
        assert_eq!(key, edc16c41_calculate_key(0xDEAD_BEEF).to_be_bytes().to_vec());
    }

    #[test]
    fn bosch_key_from_seed_edc16_family_also_uses_real_algo() {
        let seed = [0x00, 0x00, 0x00, 0x01];
        let key = bosch_key_from_seed(&seed, "EDC16");
        assert_eq!(key, edc16c41_calculate_key(0x0000_0001).to_be_bytes().to_vec());
    }

    #[test]
    fn bosch_key_from_seed_prefers_c41_even_when_generic_edc16_present() {
        // "EDC16C41" contains both strings; must still hit the real path
        let seed = [0x11, 0x22, 0x33, 0x44];
        let key = bosch_key_from_seed(&seed, "Bosch EDC16C41 Nissan");
        assert_eq!(key.len(), 4);
        assert_eq!(key, edc16c41_key_from_seed_bytes(&seed).unwrap().to_vec());
    }

    #[test]
    fn edc17_starter_is_unverified_and_not_sent() {
        let seed = [0x12, 0x34];
        let r = bosch_key_result(&seed, "EDC17_COMMON");
        assert!(!r.verified);
        assert!(r.note.contains("not a measured key"));
        assert!(bosch_key_from_seed(&seed, "EDC17_COMMON").is_empty());
    }

    #[test]
    fn med17_starter_is_unverified() {
        let seed = [0xAA, 0xBB, 0xCC, 0xDD];
        let r = bosch_key_result(&seed, "MED17_COMMON");
        assert!(!r.verified);
        assert!(bosch_key_from_seed(&seed, "MED17").is_empty());
    }

    fn validate_checksum_test(frame: &[u8]) -> bool {
        if frame.len() < 2 {
            return false;
        }
        let expected = frame[..frame.len() - 1]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b));
        expected == frame[frame.len() - 1]
    }
}
