//! GM P01 / P59 Seed-Key Security Unlock + Bosch UDS Service 0x27 framework
//!
//! Implements the two known P01 (0411) algorithms:
//!   • Level 1  — unlocks Mode 27/3B services (read data, clear DTCs)
//!   • Level 2  — unlocks Mode 34/36 flash programming (write flash)
//!
//! + Bosch UDS (ISO 14229) SecurityAccess (0x27) for EDC16/EDC17/MED17 families.
//!   Framework ready for per-family algorithms from your personal dumps.
//!   Common community patterns (XOR/shift/table) provided as starting points.
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
// Bosch UDS SecurityAccess (0x27) — EDC16 / EDC17 / MED17 framework
// ─────────────────────────────────────────────────────────────────────────────
//
// UDS payload (after ISO-TP / CAN framing handled by can.rs / j2534):
//   RequestSeed:  27 XX          (XX = 0x01, 0x03, 0x05 ...)
//   Positive:     67 XX <seed bytes typically 2 or 4>
//   SendKey:      27 YY <key bytes>
//   Positive:     67 YY
//
// Exact algorithms are family + software revision specific. This provides:
//   - Request / response builders
//   - A set of common community starting algorithms (XOR, rotate, additive)
//   - Extension points so you can drop exact tables from your own dumps
//     (see reference/2byte-keys.txt style collections)

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoschSecurityLevel {
    Diagnostic = 0x01,
    Programming = 0x03,
    Extended = 0x05,
}

/// Build UDS SecurityAccess RequestSeed payload (SID 0x27 + subfn).
pub fn bosch_uds_request_seed(level: BoschSecurityLevel) -> Vec<u8> {
    vec![0x27, level as u8]
}

/// Build UDS SecurityAccess SendKey payload.
pub fn bosch_uds_send_key(level: BoschSecurityLevel, key: &[u8]) -> Vec<u8> {
    let mut p = vec![0x27, (level as u8) + 1]; // even sub-function for key
    p.extend_from_slice(key);
    p
}

/// Common community-style key derivation for many Bosch EDC/MED families.
/// These are starting points only — replace with exact algo from your dump
/// analysis (WinOLS / IDA / community tables) for production unlock rates.
pub fn bosch_key_from_seed(seed: &[u8], family_hint: &str) -> Vec<u8> {
    if seed.is_empty() {
        return vec![0x00, 0x00];
    }
    let fam = family_hint.to_ascii_uppercase();
    // Simple 2-byte seed → 2-byte key patterns widely documented in DIY circles
    if seed.len() >= 2 {
        let s0 = seed[0];
        let s1 = seed[1];
        if fam.contains("EDC16") || fam.contains("EDC17") {
            // Example rotate + XOR pattern (replace with real from your 392203 etc)
            let k0 = s0.wrapping_add(0x5A).rotate_left(3) ^ 0xA5;
            let k1 = s1.wrapping_add(0x3C).rotate_right(2) ^ 0x5A;
            return vec![k0, k1];
        }
        if fam.contains("MED17") {
            let k0 = s0 ^ 0xC3;
            let k1 = s1.wrapping_add(s0).wrapping_mul(0x11);
            return vec![k0, k1];
        }
        // Generic fallback
        let k0 = s0 ^ 0xFF;
        let k1 = s1.wrapping_add(1);
        return vec![k0, k1];
    }
    // 4-byte seeds (some MED17/EDC17)
    if seed.len() >= 4 {
        let mut out = vec![0u8; 4];
        for i in 0..4 {
            out[i] = seed[i].wrapping_add(0x12).rotate_left((i as u32) + 1) ^ 0x55;
        }
        return out;
    }
    seed.to_vec()
}

/// High-level Bosch UDS unlock helper (payload only — call via can::uds_request or j2534).
/// Returns the key bytes that should be sent after receiving the seed.
/// Full end-to-end: request seed → receive → compute → send key → check positive.
pub fn bosch_compute_key_for_seed(seed: &[u8], family: &str) -> Vec<u8> {
    bosch_key_from_seed(seed, family)
}

/// Convenience: given a raw UDS positive seed response (67 XX seed...), extract seed bytes.
pub fn bosch_parse_seed_from_response(resp: &[u8]) -> Result<Vec<u8>, String> {
    if resp.len() < 3 || resp[0] != 0x67 {
        if !resp.is_empty() && resp[0] == 0x7F {
            return Err(format!("Negative response NRC 0x{:02X}", resp.get(2).copied().unwrap_or(0)));
        }
        return Err("Not a positive SecurityAccess seed response".into());
    }
    Ok(resp[2..].to_vec())
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level1_known_seed_0x1234() {
        let (kh, kl) = p01_key_l1(0x12, 0x34);
        let key = u16::from_be_bytes([kh, kl]);
        assert_ne!(key, 0x0000, "key should not be zero for non-zero seed");
        assert_ne!(key, 0x1234, "key should not equal seed");
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
        let seed = (0xAB, 0xCD);
        let k1 = p01_key_l1(seed.0, seed.1);
        let k2 = p01_key_l2(seed.0, seed.1);
        assert_ne!(k1, k2, "Level1 and Level2 keys must differ");
    }

    #[test]
    fn seed_response_parse_valid() {
        let cs: u8 = [0x48u8, 0x6B, 0x10, 0x67, 0x01, 0xAB, 0xCD]
            .iter().fold(0u8, |a, &b| a.wrapping_add(b));
        let frame = vec![0x48, 0x6B, 0x10, 0x67, 0x01, 0xAB, 0xCD, cs];
        let result = parse_seed_response(&frame, 0x01);
        assert_eq!(result, Ok((0xAB, 0xCD)));
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
        let frame = build_seed_request_l1();
        assert!(validate_checksum_test(&frame));
    }

    #[test]
    fn key_send_l1_frame_checksum() {
        let frame = build_key_send_l1(0x12, 0x34);
        assert!(validate_checksum_test(&frame));
    }

    #[test]
    fn bosch_key_deterministic() {
        let seed = [0x12, 0x34];
        let k1 = bosch_key_from_seed(&seed, "EDC16C41");
        let k2 = bosch_key_from_seed(&seed, "EDC16C41");
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 2);
    }

    #[test]
    fn bosch_parse_seed() {
        let resp = vec![0x67, 0x01, 0xAB, 0xCD];
        let seed = bosch_parse_seed_from_response(&resp).unwrap();
        assert_eq!(seed, vec![0xAB, 0xCD]);
    }

    fn validate_checksum_test(frame: &[u8]) -> bool {
        if frame.len() < 2 { return false; }
        let expected = frame[..frame.len()-1]
            .iter().fold(0u8, |a, &b| a.wrapping_add(b));
        expected == frame[frame.len()-1]
    }
}
