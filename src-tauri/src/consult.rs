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
use crate::transport::{elm_command, elm_request, ensure_text_mode, request_checked, AdapterGuard, ElmSession, READ_WINDOW_MS};

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

/// Data length (bytes after `62 DID`) seen in every positive read on the reference
/// Patrol (Car Scanner log, all 10 sessions): 1101..1147 one byte, 1207..130E two.
/// 1143 and 120B are advertised in the masks but were never read in any session of
/// any car (Data Logger checked all 77,994 service-22 requests), so their length is
/// unknown (None): they are only read one at a time, see `check_unknown_len`.
/// Lengths are observations, not meanings.
pub fn nissan_did_len(id: u16) -> Option<usize> {
    if !PATROL_ZD30_SUPPORTED_IDS.contains(&id) || id == 0x1143 || id == 0x120B {
        return None;
    }
    match id {
        0x1101..=0x1147 => Some(1),
        0x1207..=0x130E => Some(2),
        _ => None,
    }
}

/// DIDs per multi-DID request (as Car Scanner sends them: `22 1101 1102 1104`).
pub const NISSAN_MULTI_DIDS: usize = 3;

#[derive(Debug, Clone, serde::Serialize)]
pub struct NissanIdRead {
    pub id: String,
    pub meaning: &'static str,
    pub raw_hex: Option<String>,
    pub error: Option<String>,
    /// "multi" (3 DIDs per request) or "single".
    pub via: &'static str,
}

pub const UNKNOWN_MEANING: &str = "supported, meaning unknown";

/// Split a multi-DID positive payload (bytes after `62`) into per-DID values.
/// Each DID echo must appear in request order with its known length, and the
/// payload must end exactly after the last one; anything else is an error.
pub fn parse_multi_did(payload: &[u8], ids: &[u16]) -> Result<Vec<(u16, Vec<u8>)>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    for &id in ids {
        let len = nissan_did_len(id).ok_or_else(|| format!("{:04X}: length unknown, not read in a multi request", id))?;
        if payload.len() < i + 2 + len || payload[i] != (id >> 8) as u8 || payload[i + 1] != id as u8 {
            return Err(format!("{:04X}: echo/length mismatch in {}", id, hex(payload)));
        }
        out.push((id, payload[i + 2..i + 2 + len].to_vec()));
        i += 2 + len;
    }
    if i != payload.len() {
        return Err(format!("{} trailing byte(s) in {}", payload.len() - i, hex(payload)));
    }
    Ok(out)
}

/// `10C0`/`1081` with Car Scanner's `1` response-count suffix, only once the
/// CAN protocol is known (a response-count digit is meaningless during a search).
fn session_cmd(sess: &ElmSession, base: &str) -> String {
    if sess.protocol_found && sess.can_frames { format!("{}1", base) } else { base.to_string() }
}

/// Positive Nissan reads were only ever seen inside 10C0 -> 50C0 sent to 7E0.
fn enter_session(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) -> Result<(), ElmError> {
    let cmd = session_cmd(sess, "10C0");
    request_checked(port, sess, &cmd, READ_WINDOW_MS, 0x10, &[0xC0]).map(|_| ())
}

fn set_ecm_header(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) -> Result<(), String> {
    if !sess.header_7e0 {
        elm_command(port, &format!("ATSH{}", NISSAN_ECM_TX)).map_err(|e| format!("ATSH{}: {}", NISSAN_ECM_TX, e))?;
        sess.header_7e0 = true;
    }
    Ok(())
}

/// NRCs that mean the ECU is no longer in the session the request needs:
/// 0x7F serviceNotSupportedInActiveSession, 0x22 conditionsNotCorrect.
pub fn is_session_lost(e: &ElmError) -> bool {
    matches!(e, ElmError::Negative { nrc: 0x7F | 0x22, .. })
}

/// Re-send 10C0 mid-block (Car Scanner did this on 26 Jun) and count it.
fn reenter_session(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) -> bool {
    sess.nissan_reentries = sess.nissan_reentries.saturating_add(1);
    enter_session(port, sess).is_ok()
}

/// Length check for DIDs whose length was never observed (1143, 120B): at
/// least one byte, at most 8 (what a raw integer channel can hold). The actual
/// length is reported with the value; nothing about it is assumed.
pub fn check_unknown_len(id: u16, d: &[u8]) -> Result<(), ElmError> {
    if d.is_empty() || d.len() > 8 {
        return Err(ElmError::Unmatched(format!("{:04X}: {} byte(s), length unknown (accepted 1..=8)", id, d.len())));
    }
    Ok(())
}

fn read_single_did(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession, id: u16) -> Result<Vec<u8>, ElmError> {
    let cmd = format!("22{:04X}", id);
    let echo = [(id >> 8) as u8, id as u8];
    let mut once = |port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession| {
        elm_request(port, sess, &cmd, HeaderMode::Can11, READ_WINDOW_MS).and_then(|r| r.positive_from(Some("7E8"), 0x22, &echo))
    };
    let mut r = once(port, sess);
    // 7F 22 21 (busyRepeatRequest) was seen 4 times on single reads: retry once.
    if r == Err(ElmError::Negative { sid: 0x22, nrc: 0x21 }) {
        r = once(port, sess);
    }
    if r.as_ref().err().map(is_session_lost).unwrap_or(false) && reenter_session(port, sess) {
        r = once(port, sess);
    }
    let d = r?;
    match nissan_did_len(id) {
        Some(len) if d.len() != len => Err(ElmError::Unmatched(format!("{:04X}: {} byte(s), expected {}", id, d.len(), len))),
        Some(_) => Ok(d),
        None => check_unknown_len(id, &d).map(|_| d),
    }
}

/// Read DIDs raw. Known-length DIDs go 3 per request; if a multi request fails in
/// any way (7F 22 23, 7F 22 21, NO DATA, malformed, length mismatch) its DIDs are
/// read one at a time instead. Unknown-length DIDs are always single reads.
/// Caller must already be in the Nissan session on 7E0.
pub fn read_dids_in(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession, ids: &[u16]) -> Vec<NissanIdRead> {
    let mut out = Vec::new();
    let (known, unknown): (Vec<u16>, Vec<u16>) = ids.iter().partition(|id| nissan_did_len(**id).is_some());
    let push = |out: &mut Vec<NissanIdRead>, id: u16, r: Result<Vec<u8>, String>, via| {
        out.push(NissanIdRead {
            id: format!("{:04X}", id),
            meaning: UNKNOWN_MEANING,
            raw_hex: r.as_ref().ok().map(|d| hex(d)),
            error: r.err(),
            via,
        })
    };
    for group in known.chunks(NISSAN_MULTI_DIDS) {
        let mut multi: Result<Vec<(u16, Vec<u8>)>, String> = Err("single".into());
        if group.len() > 1 {
            let cmd: String = std::iter::once("22".to_string()).chain(group.iter().map(|i| format!("{:04X}", i))).collect();
            let r = elm_request(port, sess, &cmd, HeaderMode::Can11, READ_WINDOW_MS)
                .and_then(|r| r.positive_from(Some("7E8"), 0x22, &[]));
            if r.as_ref().err().map(is_session_lost).unwrap_or(false) {
                // Session dropped: re-enter, then the single reads below run in it.
                reenter_session(port, sess);
            }
            multi = r.map_err(|e| e.to_string()).and_then(|p| parse_multi_did(&p, group));
        }
        match multi {
            Ok(vals) => vals.into_iter().for_each(|(id, d)| push(&mut out, id, Ok(d), "multi")),
            Err(_) => {
                for &id in group {
                    let r = read_single_did(port, sess, id).map_err(|e| e.to_string());
                    push(&mut out, id, r, "single");
                }
            }
        }
    }
    for id in unknown {
        let r = read_single_did(port, sess, id).map_err(|e| e.to_string());
        push(&mut out, id, r, "single");
    }
    out
}

/// Identity DIDs (ISO 14229 F190 = VIN, F18C = ECU serial). On the reference
/// Patrol both get `7E8 03 7F 22 12` inside 10C0 (26 Jun log); `22F190` sent to
/// 7DF outside a session got NO DATA. (The 7F 22 11 in the log came from the BCM
/// on 765, not the ECU.) Only the outcome is reported; returned bytes are never
/// stored or shown. Must be called inside the 10C0 session on 7E0.
pub const NISSAN_IDENTITY_DIDS: [u16; 2] = [0xF190, 0xF18C];

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct IdentityProbe {
    pub id: String,
    pub outcome: String,
}

pub fn identity_probe_in(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) -> Vec<IdentityProbe> {
    NISSAN_IDENTITY_DIDS
        .iter()
        .map(|&id| {
            let echo = [(id >> 8) as u8, id as u8];
            let r = elm_request(port, sess, &format!("22{:04X}", id), HeaderMode::Can11, READ_WINDOW_MS)
                .and_then(|r| r.positive_from(Some("7E8"), 0x22, &echo));
            let outcome = match r {
                Ok(d) => format!("answered ({} bytes, not stored)", d.len()),
                Err(ElmError::Negative { nrc, .. }) => format!("not supported by this ECU (NRC {:02X} {})", nrc, crate::elm_parse::nrc_name(nrc)),
                Err(ElmError::NoData) => "no answer (NO DATA)".into(),
                Err(e) => format!("not read: {}", e),
            };
            IdentityProbe { id: format!("{:04X}", id), outcome }
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NissanScan {
    pub session_entered: bool,
    pub supported_ids: Vec<String>,
    pub mask_errors: Vec<String>,
    pub reads: Vec<NissanIdRead>,
    pub identity: Vec<IdentityProbe>,
    pub session_exit: String,
    pub note: &'static str,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02X}", x)).collect()
}

/// Read-only scan: enter session C0 on 7E0, read the support masks, optionally
/// read each supported ID raw (3 per request, single-read fallback) and probe
/// F190/F18C. A Drop guard always sends 1081 and ATSH7DF, also on error.
/// Fails closed: if `10C0` is not answered with `50C0`, nothing else is read.
pub fn nissan_can_scan_in(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
    read_values: bool,
) -> Result<NissanScan, String> {
    ensure_text_mode(port, sess);
    let exit_cmd = session_cmd(sess, "1081");
    let mut guard = AdapterGuard::new(port, &[exit_cmd.as_str(), "ATSH7DF"]);
    let port = guard.port();
    sess.header_7e0 = false;
    sess.nissan_open = false;
    set_ecm_header(port, sess)?;
    let mode = HeaderMode::Can11;
    let mut scan = NissanScan {
        session_entered: false,
        supported_ids: vec![],
        mask_errors: vec![],
        reads: vec![],
        identity: vec![],
        session_exit: String::new(),
        note: "Nissan CAN path observed on a 2007 Patrol ZD30CRD (cal ID 0VS43B). ID meanings are unknown; values are raw bytes only.",
    };
    let entered = enter_session(port, sess);
    let result = match entered {
        Err(e) => Err(format!("10C0 not accepted ({}). Nothing read.", e)),
        Ok(_) => {
            scan.session_entered = true;
            let mut ids: Vec<u16> = Vec::new();
            for start in NISSAN_SUPPORT_BASES {
                let mut base = start;
                loop {
                    let req = format!("22{:04X}", base);
                    match elm_request(port, sess, &req, mode, READ_WINDOW_MS).and_then(|r| r.positive_from(Some("7E8"), 0x22, &[(base >> 8) as u8, base as u8])) {
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
                scan.reads = read_dids_in(port, sess, &ids);
                scan.identity = identity_probe_in(port, sess);
            }
            Ok(())
        }
    };
    scan.session_exit = match elm_request(port, sess, &exit_cmd, mode, READ_WINDOW_MS).and_then(|r| r.positive_from(Some("7E8"), 0x10, &[0x81])) {
        Ok(_) => { guard.done(&exit_cmd); "5081 default session".into() }
        Err(e) => format!("1081 not confirmed: {} (guard resends it)", e),
    };
    drop(guard); // sends ATSH7DF (and 1081 again if it was not confirmed)
    sess.header_7e0 = false;
    result.map(|_| scan)
}

/// Samples per Nissan poll block. Car Scanner wrapped every block of value reads
/// in 10C0(1) ... 1081(1) (20.05 session: 1079 blocks in 47,636 exchanges, about
/// 44 exchanges per block). The exact block length is an approximation of that.
pub const NISSAN_BLOCK_SAMPLES: u32 = 4;

/// Poll raw values for logging channels `nissan_XXXX`, in Car Scanner's block
/// pattern: the first sample of a block sends ATSH7E0 + 10C0(1); later samples
/// only read; after `NISSAN_BLOCK_SAMPLES` samples the block closes with 1081(1).
/// If the ECU drops the session (7F xx 7F / 7F xx 22) mid-block, 10C0 is re-sent.
/// Any error path closes the session through the Drop guard (1081 + ATSH7DF).
/// Values are raw big-endian integers with unknown meaning.
pub fn nissan_poll_raw_in(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession, ids: &[u16]) -> Result<Vec<(u16, Result<u64, String>)>, String> {
    ensure_text_mode(port, sess);
    let exit_cmd = session_cmd(sess, "1081");
    let mut guard = AdapterGuard::new(port, &[exit_cmd.as_str(), "ATSH7DF"]);
    let port = guard.port();
    let fail = |sess: &mut ElmSession, e: String| {
        sess.nissan_open = false;
        sess.header_7e0 = false;
        e
    };
    if let Err(e) = set_ecm_header(port, sess) {
        return Err(fail(sess, e));
    }
    if !sess.nissan_open {
        if let Err(e) = enter_session(port, sess) {
            return Err(fail(sess, format!("10C0 not accepted ({})", e)));
        }
        sess.nissan_open = true;
        sess.nissan_block_samples = 0;
    }
    let reads = read_dids_in(port, sess, ids);
    sess.nissan_block_samples += 1;
    if sess.nissan_block_samples >= NISSAN_BLOCK_SAMPLES {
        drop(guard); // end of block: 1081(1) + ATSH7DF
        sess.nissan_open = false;
        sess.header_7e0 = false;
    } else {
        guard.done(&exit_cmd);
        guard.done("ATSH7DF");
    }
    Ok(reads
        .into_iter()
        .map(|r| {
            let id = u16::from_str_radix(&r.id, 16).unwrap_or(0);
            let v = match (r.raw_hex, r.error) {
                (Some(h), _) if !h.is_empty() && h.len() <= 16 => u64::from_str_radix(&h, 16).map_err(|e| e.to_string()),
                (Some(h), _) => Err(format!("{} bytes, too long for a raw channel", h.len() / 2)),
                (None, e) => Err(e.unwrap_or_default()),
            };
            (id, v)
        })
        .collect())
}

/// Close an open Nissan poll block (log stop, disconnect): 1081(1) + ATSH7DF.
pub fn nissan_block_end_in(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) {
    if sess.nissan_open || sess.header_7e0 {
        let exit_cmd = session_cmd(sess, "1081");
        let restore: Vec<&str> = if sess.nissan_open { vec![exit_cmd.as_str(), "ATSH7DF"] } else { vec!["ATSH7DF"] };
        drop(AdapterGuard::new(port, &restore));
        sess.nissan_open = false;
        sess.header_7e0 = false;
        sess.nissan_block_samples = 0;
    }
}

pub fn nissan_block_end(port: &mut Box<dyn SerialPort + Send>) {
    crate::transport::with_session(|sess| nissan_block_end_in(port, sess));
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
        assert_eq!(scan.identity.len(), 2);
        assert_eq!(scan.session_exit, "5081 default session");
        let sent = log.get();
        assert_eq!(sent.iter().position(|c| c == "ATSH7E0").is_some(), true);
        assert_eq!(sent.last().map(String::as_str), Some("ATSH7DF"));
        // GM IDs are never requested on the Nissan path.
        for gm in ["221141", "22125A", "221171"] {
            assert!(!sent.iter().any(|c| c == gm));
        }
    }

    const READS: &str = include_str!("../../reference/fixtures/elm_patrol_nissan_22reads.txt");

    /// DIDs Car Scanner actually polled (Data Logger Section 1a, all sessions).
    const CARSCANNER_POLLED: [u16; 30] = [
        0x1101, 0x1102, 0x1104, 0x1105, 0x1106, 0x1107, 0x1116, 0x1117, 0x111C, 0x111E,
        0x1128, 0x1129, 0x1132, 0x1133, 0x1146, 0x1147,
        0x1207, 0x1208, 0x120C, 0x120D, 0x121C, 0x121F,
        0x1302, 0x1303, 0x1305, 0x1307, 0x1308, 0x130A, 0x130D, 0x130E,
    ];

    #[test]
    fn mask_decode_is_polled_set_plus_1143_and_120b() {
        // Masks as recorded (Section 2, identical in all 10 sessions); MSB of byte 0 = base+1.
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
        let mut want: Vec<u16> = CARSCANNER_POLLED.to_vec();
        want.extend([0x1143, 0x120B]);
        want.sort();
        assert_eq!(ids, want);
        assert_eq!(decode_support_mask(0x1100, &[0x80, 0, 0, 0]), vec![0x1101]);
    }

    #[test]
    fn did_lengths_follow_observations() {
        for id in CARSCANNER_POLLED {
            let want = if id < 0x1200 { 1 } else { 2 };
            assert_eq!(nissan_did_len(id), Some(want), "{id:04X}");
        }
        assert_eq!(nissan_did_len(0x1143), None);
        assert_eq!(nissan_did_len(0x120B), None);
        assert_eq!(nissan_did_len(0x1124), None);
    }

    fn reads_sess() -> ElmSession {
        let mut s = ElmSession::for_protocol("can");
        s.p2_star_ms = 200;
        s
    }

    #[test]
    fn replay_multi_did_reads() {
        // First R1 cycle, values from the log: 1101=54 1102=0D 1104=38, 1207=33F2 1208=0B4F.
        let (mut port, log) = ReplayPort::from_fixture_merged(READS, "## Section R1").boxed();
        let mut sess = reads_sess();
        let r = read_dids_in(&mut port, &mut sess, &[0x1101, 0x1102, 0x1104, 0x1147, 0x1207, 0x1208]);
        let got: Vec<(&str, Option<&str>, &str)> = r.iter().map(|x| (x.id.as_str(), x.raw_hex.as_deref(), x.via)).collect();
        assert_eq!(got, vec![
            ("1101", Some("54"), "multi"), ("1102", Some("0D"), "multi"), ("1104", Some("38"), "multi"),
            ("1147", Some("56"), "multi"), ("1207", Some("33F2"), "multi"), ("1208", Some("0B4F"), "multi"),
        ]);
        assert!(r.iter().all(|x| x.meaning == UNKNOWN_MEANING));
        assert_eq!(log.get(), vec!["22110111021104", "22114712071208"]);
    }

    #[test]
    fn parse_multi_did_checks_echo_and_length() {
        let p = [0x11, 0x01, 0x54, 0x11, 0x02, 0x0D, 0x11, 0x04, 0x38];
        assert_eq!(parse_multi_did(&p, &[0x1101, 0x1102, 0x1104]).unwrap()[2], (0x1104, vec![0x38]));
        assert!(parse_multi_did(&p, &[0x1101, 0x1104, 0x1102]).is_err());
        assert!(parse_multi_did(&p[..8], &[0x1101, 0x1102, 0x1104]).is_err());
        assert!(parse_multi_did(&[0x11, 0x01, 0x54, 0x00], &[0x1101]).is_err());
        assert!(parse_multi_did(&p, &[0x1143]).is_err(), "unknown length never in multi");
    }

    #[test]
    fn multi_negative_falls_back_to_single_reads() {
        // Real replies: multi 22110111021104 -> 7F 22 23 and 22110511061107 -> 7F 22 21 (R1);
        // single 221101 -> 7E8 04 62 11 01 xx shape as in R2.
        let (mut port, log) = ReplayPort::empty()
            .with_reply("22110111021104", &["7E8037F2223"])
            .with_reply("22110511061107", &["7E8037F2221"])
            .with_reply("221101", &["7E80462110182"])
            .with_reply("221102", &["7E8046211022D"])
            .with_reply("221104", &["7E8046211044B"])
            .with_reply("221105", &["7E80462110556"])
            .with_reply("221106", &["7E8037F2221", "7E804621106BB"])
            .with_reply("221107", &["7E80462110767"])
            .boxed();
        let mut sess = reads_sess();
        let r = read_dids_in(&mut port, &mut sess, &[0x1101, 0x1102, 0x1104, 0x1105, 0x1106, 0x1107]);
        assert!(r.iter().all(|x| x.via == "single" && x.raw_hex.is_some()), "{r:?}");
        assert_eq!(r[4].raw_hex.as_deref(), Some("BB"), "1106 retried once after 7F 22 21");
        let sent = log.get();
        assert_eq!(sent.iter().filter(|c| *c == "221106").count(), 2);
    }

    #[test]
    fn replay_r2_mixed_reads_with_negatives() {
        // R2 has 7F2223 on multi and 7F2221 on the single 221116; nothing invented.
        let (mut port, _log) = ReplayPort::from_fixture_merged(READS, "## Section R2").boxed();
        let mut sess = reads_sess();
        let r = read_dids_in(&mut port, &mut sess, &CARSCANNER_POLLED);
        assert_eq!(r.len(), 30);
        for x in &r {
            let id = u16::from_str_radix(&x.id, 16).unwrap();
            if let Some(h) = &x.raw_hex {
                assert_eq!(h.len(), 2 * nissan_did_len(id).unwrap(), "{}", x.id);
            } else {
                assert!(x.error.is_some());
            }
        }
        assert!(r.iter().any(|x| x.raw_hex.is_some()));
    }

    #[test]
    fn unknown_length_did_and_no_data() {
        let (mut port, _log) = ReplayPort::from_fixture_merged(READS, "## Section R3").boxed();
        let mut sess = reads_sess();
        let r = read_dids_in(&mut port, &mut sess, &[0x1124]);
        assert_eq!(r[0].via, "single");
        assert_eq!(r[0].raw_hex, None);
        assert!(r[0].error.as_deref().unwrap().contains("NO DATA"));
    }

    const SESSION_ID: &str = include_str!("../../reference/fixtures/elm_patrol_session_identity.txt");

    #[test]
    fn identity_dids_real_frames() {
        // 26 Jun, verbatim: 22F190 on 7DF after 1081 -> NO DATA; inside 10C0 on 7E0
        // 22F18C and 22F190 -> 7E8 03 7F 22 12 (the 7F 22 11 in the log was the BCM on 765).
        let (mut port, _log) = ReplayPort::from_fixture(SESSION_ID, "## ITEM 1").boxed();
        let mut sess = reads_sess();
        let outside = elm_request(&mut port, &mut sess, "22F190", HeaderMode::Can11, 200).unwrap();
        assert_eq!(outside.positive_from(Some("7E8"), 0x22, &[0xF1, 0x90]), Err(ElmError::NoData));
        let p = identity_probe_in(&mut port, &mut sess);
        assert_eq!(p.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["F190", "F18C"]);
        for x in &p {
            assert!(x.outcome.starts_with("not supported by this ECU (NRC 12"), "{}: {}", x.id, x.outcome);
            assert!(!x.outcome.contains("NRC 11"));
        }
    }

    #[test]
    fn unknown_length_dids_read_singly_with_length_check() {
        // 1143/120B: never read in the log. Shapes below are synthetic (no real frames exist).
        let (mut port, log) = ReplayPort::empty()
            .with_reply("221143", &["7E80462114312"])
            .with_reply("22120B", &["7E80362120B"])
            .boxed();
        let mut sess = reads_sess();
        let r = read_dids_in(&mut port, &mut sess, &[0x1143, 0x120B]);
        assert_eq!(log.get(), vec!["221143", "22120B"], "never in a multi request");
        assert_eq!(r[0].raw_hex.as_deref(), Some("12"));
        assert!(r[1].raw_hex.is_none() && r[1].error.as_deref().unwrap().contains("length unknown"), "{:?}", r[1]);
        assert!(check_unknown_len(0x1143, &[0; 9]).is_err());
    }

    #[test]
    fn scan_error_still_restores_session_and_header() {
        // ATSH7E0 accepted, 10C0 answered only by foreign frames (11 Aug shape): fail closed.
        let (mut port, log) = ReplayPort::empty()
            .with_reply("ATSH7E0", &["OK"])
            .with_reply("10C0", &["2D1200000030E", "2D1200000010E"])
            .boxed();
        let mut sess = reads_sess();
        let e = nissan_can_scan_in(&mut port, &mut sess, true).unwrap_err();
        assert!(e.contains("10C0 not accepted"), "{e}");
        let sent = log.get();
        assert!(!sent.iter().any(|c| c.starts_with("22")));
        assert_eq!(&sent[sent.len() - 2..], &["1081", "ATSH7DF"]);
        assert!(sess.recoveries >= 1);
    }

    fn block_port() -> (Box<dyn SerialPort + Send>, crate::replay_port::SentLog) {
        ReplayPort::from_fixture_merged(READS, "## Section R1")
            .with_reply("ATSH7E0", &["OK"]).with_reply("ATSH7DF", &["OK"])
            .with_reply("10C0", &["7E80250C0"]).with_reply("1081", &["7E8025081"])
            .boxed()
    }

    #[test]
    fn raw_poll_uses_blocks_like_car_scanner() {
        let (mut port, log) = block_port();
        let mut sess = reads_sess();
        sess.protocol_found = true;
        sess.can_frames = true;
        let v = nissan_poll_raw_in(&mut port, &mut sess, &[0x1147, 0x1207, 0x1208]).unwrap();
        assert!(v.contains(&(0x1207, Ok(0x33F2))) && v.contains(&(0x1147, Ok(0x56))), "{v:?}");
        assert!(sess.nissan_open);
        for _ in 1..NISSAN_BLOCK_SAMPLES {
            nissan_poll_raw_in(&mut port, &mut sess, &[0x1147, 0x1207, 0x1208]).unwrap();
        }
        let sent = log.get();
        // One 10C0(1) at the start of the block, one 1081(1) at the end, nothing per sample.
        assert_eq!(sent.iter().filter(|c| c.starts_with("10C0")).count(), 1);
        assert_eq!(sent.iter().filter(|c| c.starts_with("1081")).count(), 1);
        assert_eq!(sent.iter().filter(|c| *c == "22114712071208").count(), NISSAN_BLOCK_SAMPLES as usize);
        let sh = sent.iter().position(|c| c == "ATSH7E0").unwrap();
        assert_eq!(sent[sh + 1], "10C01", "suffix once CAN is known");
        assert_eq!(&sent[sent.len() - 2..], &["10811", "ATSH7DF"]);
        assert!(!sess.nissan_open && !sess.header_7e0);
    }

    #[test]
    fn block_end_closes_open_session() {
        let (mut port, log) = block_port();
        let mut sess = reads_sess();
        nissan_poll_raw_in(&mut port, &mut sess, &[0x1147, 0x1207, 0x1208]).unwrap();
        assert!(log.get().iter().any(|c| c == "10C0"), "no suffix before the protocol is known");
        nissan_block_end_in(&mut port, &mut sess);
        let sent = log.get();
        assert_eq!(&sent[sent.len() - 2..], &["1081", "ATSH7DF"]);
        assert!(!sess.nissan_open);
    }

    #[test]
    fn session_drop_mid_block_resends_10c0() {
        // Car Scanner re-sent 10C01 mid-cycle on 26 Jun. The 7F 22 7F reply below is
        // synthetic (NRC 7F = serviceNotSupportedInActiveSession); the log does not quote one.
        let (mut port, log) = ReplayPort::empty()
            .with_reply("ATSH7E0", &["OK"]).with_reply("ATSH7DF", &["OK"])
            .with_reply("10C0", &["7E80250C0"]).with_reply("1081", &["7E8025081"])
            .with_reply("221101", &["7E80462110156", "7E8037F227F", "7E80462110157"])
            .boxed();
        let mut sess = reads_sess();
        let v = nissan_poll_raw_in(&mut port, &mut sess, &[0x1101]).unwrap();
        assert_eq!(v, vec![(0x1101, Ok(0x56))]);
        let v = nissan_poll_raw_in(&mut port, &mut sess, &[0x1101]).unwrap();
        assert_eq!(v, vec![(0x1101, Ok(0x57))]);
        assert_eq!(sess.nissan_reentries, 1);
        assert_eq!(log.get().iter().filter(|c| *c == "10C0").count(), 2);
        assert!(!log.get().iter().any(|c| c == "1081"), "re-entry without leaving");
    }

    #[test]
    fn poll_error_closes_session_via_guard() {
        // 10C0 refused (real Patrol-style 7F 10 12 shape from the refused-session test): guard sends 1081 + ATSH7DF.
        let (mut port, log) = ReplayPort::empty()
            .with_reply("ATSH7E0", &["OK"]).with_reply("10C0", &["7E8037F1012"])
            .boxed();
        let mut sess = reads_sess();
        assert!(nissan_poll_raw_in(&mut port, &mut sess, &[0x1101]).is_err());
        let sent = log.get();
        assert_eq!(&sent[sent.len() - 2..], &["1081", "ATSH7DF"]);
        assert!(!sess.nissan_open && !sess.header_7e0);
        assert!(!sent.iter().any(|c| c.starts_with("22")));
    }

    #[test]
    fn db_observations_match_length_table() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../reference/ecu_database/edc16c41_nissan_patrol.json")).unwrap();
        let dids = v["nissan_did_observations"]["dids"].as_object().unwrap();
        assert_eq!(dids.len(), 32);
        for id in PATROL_ZD30_SUPPORTED_IDS {
            let e = &dids[&format!("{:04X}", id)];
            assert_eq!(e["meaning"], "unknown");
            assert_eq!(e["len"].as_u64().map(|x| x as usize), nissan_did_len(id), "{id:04X}");
            if let (Some(lo), Some(hi)) = (e["min_raw"].as_str(), e["max_raw"].as_str()) {
                assert_eq!(lo.len(), 2 * nissan_did_len(id).unwrap());
                assert!(u32::from_str_radix(lo, 16).unwrap() <= u32::from_str_radix(hi, 16).unwrap());
            } else {
                assert_eq!(e["positive_reads"], 0);
                assert_eq!(e["note"], "advertised in mask, never read; length unknown");
                assert!(e["len"].is_null());
            }
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
