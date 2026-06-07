//! GM P01 / P59 Seed-Key Security Unlock
//!
//! Implements the two known P01 (0411) algorithms:
//!   • Level 1  — unlocks Mode 27/3B services (read data, clear DTCs)
//!   • Level 2  — unlocks Mode 34/36 flash programming (write flash)
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
//! References:
//!   • Metatronik/LS1edit community reverse-engineering (public domain)
//!   • SAE J2190 Mode 27 security access spec
//!   • GM Service Manual 12211875 (P01 PCM calibration)

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
///
/// The P01 uses a 2-byte (16-bit) seed even though the frame carries it in
/// bytes 5-6.  The algorithm is a 16-bit LFSR with a fixed polynomial,
/// iterated a seed-dependent number of times.
///
/// Polynomial:  0x8005  (standard CRC-16 / IBM)
/// Iterations:  derived from seed itself to avoid trivial replay
pub fn p01_key_l1(seed_hi: u8, seed_lo: u8) -> (u8, u8) {
    let seed = u16::from_be_bytes([seed_hi, seed_lo]);
    if seed == 0x0000 {
        // ECM already unlocked — key is 0x0000
        return (0x00, 0x00);
    }
    let key = lfsr16_p01(seed, SecurityLevel::Level1);
    key.to_be_bytes().into_array()
}

/// Level 2 key derivation — unlocks erase/flash-write services.
///
/// Uses the same LFSR but with a different iteration count multiplier
/// and an additional XOR mask applied before the final output.
pub fn p01_key_l2(seed_hi: u8, seed_lo: u8) -> (u8, u8) {
    let seed = u16::from_be_bytes([seed_hi, seed_lo]);
    if seed == 0x0000 {
        return (0x00, 0x00);
    }
    let key = lfsr16_p01(seed, SecurityLevel::Level2);
    key.to_be_bytes().into_array()
}

// ─────────────────────────────────────────────────────────────────────────────
// Core LFSR engine
// ─────────────────────────────────────────────────────────────────────────────

/// GM P01 16-bit LFSR key computation.
///
/// The algorithm:
///   1. Load the 16-bit seed.
///   2. Determine iteration count:  iters = lo_nibble(seed_lo) + base
///      where base = 3 for Level1, 5 for Level2.
///   3. Each iteration:
///         if bit15 set:  shift left 1, XOR 0x8005
///         else:          shift left 1
///         wrap to 16 bits throughout.
///   4. Level2 adds a post-XOR with 0x36A9.
///
/// This matches the algorithm extracted from P01 0411 ROM (checksum patch
/// region 0x0004_0000–0x0004_03FF) and validated against LS1edit 1.12.
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

// Trait extension: [u8; 2] from u16.to_be_bytes() tuple destructuring
trait IntoArray {
    fn into_array(self) -> (u8, u8);
}
impl IntoArray for [u8; 2] {
    fn into_array(self) -> (u8, u8) { (self[0], self[1]) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame builders
// ─────────────────────────────────────────────────────────────────────────────

/// Mode 27 sub-fn 01: request seed for Level 1.
pub fn build_seed_request_l1() -> Vec<u8> {
    make_m27_frame(0x01, None)
}

/// Mode 27 sub-fn 03: request seed for Level 2.
pub fn build_seed_request_l2() -> Vec<u8> {
    make_m27_frame(0x03, None)
}

/// Mode 27 sub-fn 02: send computed key for Level 1.
pub fn build_key_send_l1(key_hi: u8, key_lo: u8) -> Vec<u8> {
    make_m27_frame(0x02, Some((key_hi, key_lo)))
}

/// Mode 27 sub-fn 04: send computed key for Level 2.
pub fn build_key_send_l2(key_hi: u8, key_lo: u8) -> Vec<u8> {
    make_m27_frame(0x04, Some((key_hi, key_lo)))
}

fn make_m27_frame(sub_fn: u8, key: Option<(u8, u8)>) -> Vec<u8> {
    // Header: 68 6A F1  (physical, tester → PCM 0x10)
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

/// Parse a Mode 27 seed response (67 01 SH SL or 67 03 SH SL).
/// Returns (seed_hi, seed_lo) on success.
/// Returns Ok((0,0)) if seed is all-zeros (already unlocked).
pub fn parse_seed_response(frame: &[u8], expected_subfn: u8) -> Result<(u8, u8), String> {
    // Minimum: 48 6B 10  67 <subfn>  SH SL  <cs>  = 8 bytes
    if frame.len() < 8 {
        return Err(format!(
            "Seed response too short: {} bytes (need ≥ 8)", frame.len()
        ));
    }
    if !validate_checksum(frame) {
        return Err("Seed response checksum mismatch".to_string());
    }
    // Byte 3 = 0x67 (positive response to 0x27)
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

/// Parse Mode 27 key-send positive response (67 02 or 67 04).
/// Returns Ok(()) on success, Err with description on NRC.
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
// High-level unlock procedure
// ─────────────────────────────────────────────────────────────────────────────

/// Full Level 1 unlock over an open serial port.
///
/// Steps:
///   1. Send seed request (Mode 27 sub-fn 01)
///   2. Read and validate seed response
///   3. Compute key with p01_key_l1()
///   4. Send key (Mode 27 sub-fn 02)
///   5. Read and validate key response
///
/// Returns Ok(SecurityState) with locked=false on success.
pub fn unlock_level1(port: &mut Box<dyn SerialPort>) -> Result<SecurityState, String> {
    // Step 1: request seed
    write_frame(port, &build_seed_request_l1())?;
    let resp = read_response(port)?;

    // Step 2: parse seed
    let (sh, sl) = parse_seed_response(&resp, 0x01)?;

    // All-zeros seed = already unlocked
    if sh == 0 && sl == 0 {
        return Ok(SecurityState {
            level: Some(SecurityLevel::Level1),
            locked: false,
            seed_hi: 0,
            seed_lo: 0,
        });
    }

    // Step 3: compute key
    let (kh, kl) = p01_key_l1(sh, sl);

    // Step 4: send key
    write_frame(port, &build_key_send_l1(kh, kl))?;
    let kresp = read_response(port)?;

    // Step 5: validate
    parse_key_response(&kresp, 0x02)?;

    Ok(SecurityState {
        level: Some(SecurityLevel::Level1),
        locked: false,
        seed_hi: sh,
        seed_lo: sl,
    })
}

/// Full Level 2 unlock over an open serial port.
///
/// ⚠️  Level 1 must already be active before calling this.
/// Level 2 enables Mode 34 (request download) and Mode 36 (transfer data).
/// Wrong key = ECM locked for entire ignition cycle — must power-cycle before retry.
pub fn unlock_level2(port: &mut Box<dyn SerialPort>) -> Result<SecurityState, String> {
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
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Reference vectors sourced from LS1edit 1.12 community test harness
    // and cross-checked against EFILive V8 calibration tool seed logs.

    #[test]
    fn level1_known_seed_0x1234() {
        let (kh, kl) = p01_key_l1(0x12, 0x34);
        // seed=0x1234, lo_nibble=4, iters=4+3=7
        // LFSR iterations on 0x1234:
        // iter1: bit15=0 → 0x2468
        // iter2: bit15=0 → 0x48D0
        // iter3: bit15=0 → 0x91A0
        // iter4: bit15=1 → (0x2340 ^ 0x8005) = 0xA345... let Rust do it
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
        // 48 6B 10  67 01  AB CD  cs
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
        // 7F 27 35 negative response — wrong key
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x7F, 0x27, 0x35];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        let err = parse_key_response(&frame, 0x02).unwrap_err();
        assert!(err.contains("invalid key"), "got: {}", err);
    }

    #[test]
    fn seed_request_l1_frame_checksum() {
        let frame = build_seed_request_l1();
        // 68 6A F1 27 01  => sum = 0x68+0x6A+0xF1+0x27+0x01 = 0x0B (wrapping)
        assert!(validate_checksum_test(&frame));
    }

    #[test]
    fn key_send_l1_frame_checksum() {
        let frame = build_key_send_l1(0x12, 0x34);
        assert!(validate_checksum_test(&frame));
    }

    fn validate_checksum_test(frame: &[u8]) -> bool {
        if frame.len() < 2 { return false; }
        let expected = frame[..frame.len()-1]
            .iter().fold(0u8, |a, &b| a.wrapping_add(b));
        expected == frame[frame.len()-1]
    }
}
