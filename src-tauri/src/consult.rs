#![allow(unused, dead_code)]
//! consult.rs — Nissan diagnostics.
//!
//! Two paths:
//!
//! 1. **Nissan CAN diagnostic path (observed, read-only).** What Car Scanner did on
//!    a 2007 Patrol GU ZD30CRD over an OBDLink MX+ (11-bit CAN 500k): `ATSH7E0`,
//!    `10C0` → `50C0`, `22xxxx` reads, `1081` → `5081` back to the default
//!    session. The ECU answers `2211xx`/`2212xx`/`2213xx` "support" requests with
//!    4-byte bitmasks in the same layout as OBD Mode 01 (`221100` → `DE000615`
//!    = 1101 1102 1104-1107 1116 1117 111C 111E, chain bit 1120). Which value
//!    each ID carries is **unknown**: IDs are recorded as "supported, meaning
//!    unknown" and only raw bytes are captured. No scaling is assigned.
//!
//! 2. **Legacy K-line Consult code below — UNVERIFIED.** It guesses at a
//!    9600-baud register protocol. It has never been confirmed on any vehicle,
//!    and the reference Patrol does its diagnostics over CAN (path 1), so do not
//!    rely on it. Kept for reference only.
//!
//! Nissan Patrol GU ZD30CRD (EDC16C41) often supports both CAN and the older Consult port.
//! Consult II is a Nissan-proprietary serial protocol at 9600 baud 8N1 (sometimes 38400 on later).
//!
//! Frame style (typical observed from community + reference tools):
//!   Many commands are sent as 0x5A <subcmd> <register or data>
//!   Responses come back with echoed command + data.
//!
//! This module provides basic init + register read + raw command for the user's ZD30CRD.
//! Full memory access / flashing usually moves to CAN/UDS on these ECUs, but Consult is great
//! for live data and some older functions.

use serialport::SerialPort;
use std::time::Duration;
use crate::write_frame;

/// Common Consult baud for most Nissan ECUs
pub const CONSULT_BAUD: u32 = 9600;

/// Basic init / wake for Consult port.
/// Many devices just need to be opened at 9600 and a couple of commands sent.
pub fn consult_init(port: &mut Box<dyn SerialPort + Send>) -> Result<(), String> {
    // Some implementations send 0xFF or a specific wake.
    let wake = [0xFFu8];
    let _ = write_frame(port, &wake);
    std::thread::sleep(Duration::from_millis(100));

    // Try a harmless read (ECU ID or something)
    let _ = consult_send_command(port, 0xD0, &[]); // many firmwares accept
    Ok(())
}

/// Send a Consult command and return raw response.
/// Format is very device specific; this is the low-level primitive.
pub fn consult_send_command(
    port: &mut Box<dyn SerialPort + Send>,
    cmd: u8,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let mut frame = vec![cmd];
    frame.extend_from_slice(data);
    // Some variants add length or terminator. We keep simple.
    write_frame(port, &frame)?;
    std::thread::sleep(Duration::from_millis(40));

    let mut buf = [0u8; 128];
    let n = port.read(&mut buf).unwrap_or(0);
    Ok(buf[..n].to_vec())
}

/// Read a "register" using the common 0x5A style (very common in Nissan data logging).
/// register: the known Nissan consult register address (from community lists or WinOLS).
pub fn consult_read_register(port: &mut Box<dyn SerialPort + Send>, reg: u8) -> Result<u16, String> {
    // Typical: 0x5A 0x01 0x<reg>   or 0x5A <len> <reg>
    let req = vec![0x5A, 0x01, reg];
    let resp = consult_send_command(port, 0x5A, &req[1..])?;

    // Response often echoes and then gives 1 or 2 bytes of data.
    if resp.len() >= 2 {
        // crude big-endian or direct
        let val = ((resp[0] as u16) << 8) | (resp.get(1).copied().unwrap_or(0) as u16);
        return Ok(val);
    }
    // Fallback: return first byte as value
    Ok(resp.get(0).copied().unwrap_or(0) as u16)
}

/// UNVERIFIED guesses (legacy K-line path). Not used by the CAN path; do not treat as real
/// register meanings.
pub const REG_RPM: u8 = 0x00;        // typical
pub const REG_BOOST: u8 = 0x0B;      // or MAP
pub const REG_MAF: u8 = 0x0C;
pub const REG_INJ_PULSE: u8 = 0x1C;
pub const REG_RAIL_PRESS: u8 = 0x23;

/// Convenience: read a few key diesel params.
pub fn consult_read_basic_diesel_data(port: &mut Box<dyn SerialPort + Send>) -> Result<serde_json::Value, String> {
    let rpm = consult_read_register(port, REG_RPM).unwrap_or(0);
    let boost_raw = consult_read_register(port, REG_BOOST).unwrap_or(0);
    let maf = consult_read_register(port, REG_MAF).unwrap_or(0);

    Ok(serde_json::json!({
        "rpm": rpm,
        "boost_raw": boost_raw,
        "maf_raw": maf,
        "note": "Scale factors depend on exact ZD30 calibration. Cross reference with your bin or TunerPro/Consult logger defs."
    }))
}

pub const CONSULT_KLINE_VERIFIED: bool = false;

// ── Nissan CAN diagnostic path (observed) ──────────────────────────────────

use crate::elm_parse::{decode_support_mask, ElmError, HeaderMode};
use crate::transport::{elm_command, elm_request, ensure_text_mode, ElmSession, READ_WINDOW_MS};

/// Physical request ID used by Car Scanner for the Patrol ECM (response on 7E8).
pub const NISSAN_ECM_TX: &str = "7E0";
/// Support-mask bases Car Scanner probed on the Patrol. Others were not probed.
pub const NISSAN_SUPPORT_BASES: [u16; 3] = [0x1100, 0x1200, 0x1300];

/// Data IDs the reference Patrol (cal ID 0VS43B) listed as supported via its
/// 2211xx/2212xx/2213xx masks. Meanings are unknown.
pub const PATROL_ZD30_SUPPORTED_IDS: [u16; 32] = [
    0x1101, 0x1102, 0x1104, 0x1105, 0x1106, 0x1107, 0x1116, 0x1117, 0x111C, 0x111E,
    0x1128, 0x1129, 0x1132, 0x1133, 0x1143, 0x1146, 0x1147,
    0x1207, 0x1208, 0x120B, 0x120C, 0x120D, 0x121C, 0x121F,
    0x1302, 0x1303, 0x1305, 0x1307, 0x1308, 0x130A, 0x130D, 0x130E,
];

/// Decode one Nissan support mask: data IDs (multiples of 0x20 are chain bits,
/// not data) and whether the next mask (`base + 0x20`) is advertised.
pub fn nissan_ids_from_mask(base: u16, mask: &[u8]) -> (Vec<u16>, bool) {
    let all = decode_support_mask(base, mask);
    let next = all.contains(&(base + 0x20));
    (all.into_iter().filter(|id| id % 0x20 != 0).collect(), next)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NissanIdRead {
    pub id: String,
    pub meaning: &'static str,
    pub raw_hex: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NissanScan {
    pub session_entered: bool,
    pub supported_ids: Vec<String>,
    pub mask_errors: Vec<String>,
    pub reads: Vec<NissanIdRead>,
    pub session_exit: String,
    pub note: &'static str,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02X}", x)).collect()
}

/// Read-only scan: enter session C0 on 7E0, read the support masks, optionally
/// read each supported ID raw, then always send 1081 and restore ATSH7DF.
/// Fails closed: if `10C0` is not answered with `50C0`, nothing else is sent
/// except the session exit and header restore.
pub fn nissan_can_scan_in(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
    read_values: bool,
) -> Result<NissanScan, String> {
    ensure_text_mode(port, sess);
    elm_command(port, &format!("ATSH{}", NISSAN_ECM_TX)).map_err(|e| format!("ATSH{}: {}", NISSAN_ECM_TX, e))?;
    let mode = HeaderMode::Can11;
    let mut scan = NissanScan {
        session_entered: false,
        supported_ids: vec![],
        mask_errors: vec![],
        reads: vec![],
        session_exit: String::new(),
        note: "Nissan CAN path observed on a 2007 Patrol ZD30CRD (cal ID 0VS43B). ID meanings are unknown; values are raw bytes only.",
    };
    let entered = elm_request(port, sess, "10C0", mode, READ_WINDOW_MS).and_then(|r| r.positive(0x10, &[0xC0]));
    let result = match entered {
        Err(e) => Err(format!("10C0 not accepted ({}). Nothing read.", e)),
        Ok(_) => {
            scan.session_entered = true;
            let mut ids: Vec<u16> = Vec::new();
            for start in NISSAN_SUPPORT_BASES {
                let mut base = start;
                loop {
                    let req = format!("22{:04X}", base);
                    match elm_request(port, sess, &req, mode, READ_WINDOW_MS).and_then(|r| r.positive(0x22, &[(base >> 8) as u8, base as u8])) {
                        Ok(mask) if mask.len() >= 4 => {
                            let (found, next) = nissan_ids_from_mask(base, &mask[..4]);
                            ids.extend(found);
                            if next && (base & 0xFF) < 0xE0 { base += 0x20; } else { break; }
                        }
                        Ok(mask) => { scan.mask_errors.push(format!("{}: short mask {}", req, hex(&mask))); break; }
                        Err(e) => { scan.mask_errors.push(format!("{}: {}", req, e)); break; }
                    }
                }
            }
            scan.supported_ids = ids.iter().map(|i| format!("{:04X}", i)).collect();
            if read_values {
                for id in &ids {
                    let r = elm_request(port, sess, &format!("22{:04X}", id), mode, READ_WINDOW_MS)
                        .and_then(|r| r.positive(0x22, &[(id >> 8) as u8, *id as u8]));
                    scan.reads.push(NissanIdRead {
                        id: format!("{:04X}", id),
                        meaning: "supported, meaning unknown",
                        raw_hex: r.as_ref().ok().map(|d| hex(d)),
                        error: r.err().map(|e| e.to_string()),
                    });
                }
            }
            Ok(())
        }
    };
    // Always try to return to the default session and the functional OBD header.
    scan.session_exit = match elm_request(port, sess, "1081", mode, READ_WINDOW_MS).and_then(|r| r.positive(0x10, &[0x81])) {
        Ok(_) => "5081 default session".into(),
        Err(e) => format!("1081 not confirmed: {}", e),
    };
    let _ = elm_command(port, "ATSH7DF");
    result.map(|_| scan)
}

pub fn nissan_can_scan(port: &mut Box<dyn SerialPort + Send>, read_values: bool) -> Result<NissanScan, String> {
    crate::transport::with_session(|sess| nissan_can_scan_in(port, sess, read_values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_port::ReplayPort;
    const FIXTURE: &str = include_str!("../../reference/fixtures/elm_patrol_zd30_obdlink.txt");

    #[test]
    fn real_patrol_masks_give_the_observed_ids() {
        let mut ids = Vec::new();
        for (base, mask) in [
            (0x1100u16, [0xDE, 0x00, 0x06, 0x15]),
            (0x1120, [0x01, 0x80, 0x60, 0x01]),
            (0x1140, [0x26, 0x00, 0x00, 0x00]),
            (0x1200, [0x03, 0x38, 0x00, 0x12]),
            (0x1300, [0x6B, 0x4C, 0x00, 0x00]),
        ] {
            ids.extend(nissan_ids_from_mask(base, &mask).0);
        }
        assert_eq!(ids, PATROL_ZD30_SUPPORTED_IDS.to_vec());
        // 1209 is not in the mask, and the ECU rejects 221209 with 7F 22 12.
        assert!(!ids.contains(&0x1209));
        assert!(nissan_ids_from_mask(0x1100, &[0xDE, 0x00, 0x06, 0x15]).1, "chain to 1120");
        assert!(!nissan_ids_from_mask(0x1140, &[0x26, 0, 0, 0]).1);
    }

    #[test]
    fn replay_scan_reads_masks_and_exits_session() {
        let (mut port, log) = ReplayPort::from_fixture(FIXTURE, "## Section A1").boxed();
        let mut sess = ElmSession::for_protocol("can");
        let scan = nissan_can_scan_in(&mut port, &mut sess, true).unwrap();
        assert!(scan.session_entered);
        let want: Vec<String> = PATROL_ZD30_SUPPORTED_IDS.iter().map(|i| format!("{:04X}", i)).collect();
        assert_eq!(scan.supported_ids, want);
        // The fixture has no value reads: every ID must be reported as an error, not invented.
        assert_eq!(scan.reads.len(), 32);
        assert!(scan.reads.iter().all(|r| r.raw_hex.is_none() && r.error.is_some() && r.meaning == "supported, meaning unknown"));
        assert_eq!(scan.session_exit, "5081 default session");
        let sent = log.get();
        assert_eq!(sent.iter().position(|c| c == "ATSH7E0").is_some(), true);
        assert_eq!(sent.last().map(String::as_str), Some("ATSH7DF"));
        // GM IDs are never requested on the Nissan path.
        for gm in ["221141", "22125A", "221171"] {
            assert!(!sent.iter().any(|c| c == gm));
        }
    }

    #[test]
    fn refused_session_reads_nothing() {
        let (mut port, log) = ReplayPort::empty().with_reply("ATSH7E0", &["OK"]).with_reply("10C0", &["7E8037F1012"]).boxed();
        let mut sess = ElmSession::for_protocol("can");
        let e = nissan_can_scan_in(&mut port, &mut sess, true).unwrap_err();
        assert!(e.contains("10C0 not accepted"), "{e}");
        assert!(!log.get().iter().any(|c| c.starts_with("22")));
        assert!(log.get().iter().any(|c| c == "1081"));
    }

    #[test]
    fn negative_221209_is_an_error() {
        let (mut port, _log) = ReplayPort::from_fixture(FIXTURE, "## Section A1").boxed();
        let mut sess = ElmSession::for_protocol("can");
        let r = elm_request(&mut port, &mut sess, "221209", HeaderMode::Can11, 100).unwrap();
        assert_eq!(r.positive(0x22, &[0x12, 0x09]), Err(ElmError::Negative { sid: 0x22, nrc: 0x12 }));
    }
}
