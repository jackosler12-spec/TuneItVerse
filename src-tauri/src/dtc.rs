//! GM P01 DTC Read / Clear Engine
//!
//! Implements OBD-II / GM-specific DTC services over J1850 VPW for the
//! P01 (0411) PCM.
//!
//! ─── Services implemented ──────────────────────────────────────────────────
//!
//!  Mode  SID   Description
//!  ────  ───   ─────────────────────────────────────────────────────────────
//!  0x03  0x43  Read stored (confirmed) DTCs          — SAE J1979
//!  0x07  0x47  Read pending (test-failed) DTCs        — SAE J1979
//!  0x0A  0x4A  Read permanent DTCs                    — SAE J1979 (if supported)
//!  0x04  0x44  Clear DTCs + reset readiness monitors  — SAE J1979
//!  0x02  0x42  Read freeze frame data                 — SAE J1979
//!
//!  GM also echoes stored DTCs via Mode 43 (GM proprietary) on VPW;
//!  we send standard Mode 03 and accept both 0x43 responses.
//!
//! ─── DTC byte encoding (J2012 / ISO 15031-6) ───────────────────────────────
//!
//!  Each DTC is 2 bytes:  [B1] [B2]
//!
//!  B1 bits 7-6:  system prefix
//!    00 → P (powertrain)
//!    01 → C (chassis)
//!    10 → B (body)
//!    11 → U (network)
//!
//!  B1 bits 5-4:  first digit after prefix
//!  B1 bits 3-0:  second digit
//!  B2 bits 7-4:  third digit
//!  B2 bits 3-0:  fourth digit
//!
//!  Example: 0x01 0x23  → P0123
//!           0x43 0x00  → P1300   (note: bits 7-6 = 01 → ... wait, see below)
//!
//!  Correct decode:
//!    prefix  = (B1 >> 6) & 0x03  → 0=P, 1=C, 2=B, 3=U
//!    digit1  = (B1 >> 4) & 0x03  (0-3 only, ORed with system)
//!    digit2  = (B1 >> 0) & 0x0F
//!    digit3  = (B2 >> 4) & 0x0F
//!    digit4  = (B2 >> 0) & 0x0F
//!    code = format!("{}{}{}{}{}{}", prefix_char, digit1, digit2, digit3, digit4)
//!
//! ─── J1850 VPW frame format ────────────────────────────────────────────────
//!
//!  Request:  68 6A F1  <SID>           <cs>
//!  Response: 48 6B 10  <SID+0x40>  <DTC_bytes...>  <cs>
//!
//!  Multi-frame: P01 may return up to ~6 DTCs per frame (12 bytes).
//!  If more, it sends multiple frames — we loop until no more data.
//!
//! References:
//!   • SAE J1979 / ISO 15031-5 — OBD-II service modes
//!   • SAE J2012 — DTC classification
//!   • GM SI Document: GMLAN Diagnostic Services (P01 supplement)
//!   • EFILive / HPTuners community DTC tables

#![allow(unused_variables, dead_code, non_snake_case)]
use crate::{write_frame, read_response, validate_checksum};
use serialport::SerialPort;
use serde::Serialize;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// A single decoded DTC.
#[derive(Debug, Clone, Serialize)]
pub struct DtcRecord {
    /// Human-readable code string e.g. "P0300"
    pub code:        String,
    /// Raw 2 bytes from the ECM response
    pub raw:         [u8; 2],
    /// Short description (best-effort from built-in table)
    pub description: String,
    /// True if this came from Mode 07 (pending / test-failed this drive cycle)
    pub is_pending:  bool,
    /// True if this came from Mode 0A (permanent — cannot be cleared by Mode 04)
    pub is_permanent: bool,
}

/// Result of a full DTC read (all three mode groups).
#[derive(Debug, Clone, Serialize)]
pub struct DtcReadResult {
    /// Confirmed / stored DTCs (Mode 03)
    pub stored:    Vec<DtcRecord>,
    /// Pending DTCs — set this drive cycle but not yet confirmed (Mode 07)
    pub pending:   Vec<DtcRecord>,
    /// Permanent DTCs — set and cannot be cleared without repair (Mode 0A)
    pub permanent: Vec<DtcRecord>,
    /// Total count across all three groups
    pub total:     usize,
}

/// Result of a DTC clear operation (Mode 04).
#[derive(Debug, Clone, Serialize)]
pub struct DtcClearResult {
    /// True if the ECM acknowledged the clear
    pub success:        bool,
    /// Number of DTCs that were present before clear
    pub cleared_count:  usize,
    /// Any message from the ECM (NRC text if failed)
    pub message:        String,
}

/// Freeze frame record — snapshot of sensor data when a DTC was set.
#[derive(Debug, Clone, Serialize)]
pub struct FreezeFrameResult {
    /// DTC that triggered the freeze frame (Mode 02 PID 0x02)
    pub trigger_dtc:   Option<String>,
    pub engine_load:   Option<f32>,  // %
    pub ect_c:         Option<f32>,  // °C
    pub map_kpa:       Option<f32>,  // kPa
    pub rpm:           Option<f32>,  // RPM
    pub vss_kph:       Option<f32>,  // km/h
    pub spark_adv_deg: Option<f32>,  // degrees
    pub iat_c:         Option<f32>,  // °C
    pub maf_gs:        Option<f32>,  // g/s
    pub tps_pct:       Option<f32>,  // %
    pub stft_b1_pct:   Option<f32>,  // %
    pub ltft_b1_pct:   Option<f32>,  // %
}

// ─────────────────────────────────────────────────────────────────────────────
// DTC byte decoder
// ─────────────────────────────────────────────────────────────────────────────

/// Decode a 2-byte J2012 DTC into a human-readable code string.
///
/// Returns None if both bytes are 0x00 (no DTC / padding).
pub fn decode_dtc_bytes(b1: u8, b2: u8) -> Option<String> {
    if b1 == 0x00 && b2 == 0x00 {
        return None;
    }
    let prefix = match (b1 >> 6) & 0x03 {
        0 => 'P',
        1 => 'C',
        2 => 'B',
        3 => 'U',
        _ => unreachable!(),
    };
    let d1 = (b1 >> 4) & 0x03;
    let d2 =  b1        & 0x0F;
    let d3 = (b2 >> 4) & 0x0F;
    let d4 =  b2        & 0x0F;
    Some(format!("{}{}{}{}{}", prefix, d1, d2, d3, d4))
}

/// Build a DtcRecord from raw bytes, flagging pending/permanent as requested.
fn make_record(b1: u8, b2: u8, is_pending: bool, is_permanent: bool) -> Option<DtcRecord> {
    let code = decode_dtc_bytes(b1, b2)?;
    let description = describe_dtc(&code);
    Some(DtcRecord { code, raw: [b1, b2], description, is_pending, is_permanent })
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame builders
// ─────────────────────────────────────────────────────────────────────────────

fn build_request(sid: u8) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, sid];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

/// Mode 02 freeze frame request: SID=0x02, FF#=0x00, PID=pid
fn build_mode02_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x02, 0x00, pid];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

// ─────────────────────────────────────────────────────────────────────────────
// Response parsers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a Mode 03/07/0A response frame into DTC byte pairs.
/// Returns an empty vec if the ECM reports "no DTCs" (0x43/0x47/0x4A 0x00).
fn parse_dtc_response(frame: &[u8], expected_resp_sid: u8) -> Result<Vec<[u8; 2]>, String> {
    // Check for NRC 0x7F
    if frame.len() >= 6 && frame[3] == 0x7F {
        let nrc = frame[5];
        // NRC 0x11 = serviceNotSupported (Mode 0A not supported — treat as empty)
        if nrc == 0x11 || nrc == 0x12 {
            return Ok(vec![]);
        }
        return Err(format!("DTC read NRC 0x{:02X}: {}", nrc, nrc_text(nrc)));
    }
    if !validate_checksum(frame) {
        return Err("DTC response checksum mismatch".to_string());
    }
    if frame.len() < 5 {
        return Err(format!("DTC response too short: {} bytes", frame.len()));
    }
    if frame[3] != expected_resp_sid {
        return Err(format!(
            "DTC: expected SID 0x{:02X}, got 0x{:02X}",
            expected_resp_sid, frame[3]
        ));
    }
    // Byte 4 = DTC count (may be 0)
    let count = frame[4] as usize;
    if count == 0 {
        return Ok(vec![]);
    }
    // DTC data starts at byte 5, 2 bytes each, last byte is checksum
    let data = &frame[5..frame.len() - 1];
    let mut pairs = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() && pairs.len() < count {
        pairs.push([data[i], data[i + 1]]);
        i += 2;
    }
    Ok(pairs)
}

/// Parse a Mode 04 (0x44) clear response.
fn parse_clear_response(frame: &[u8]) -> Result<(), String> {
    if frame.len() >= 6 && frame[3] == 0x7F && frame[4] == 0x04 {
        let nrc = frame[5];
        return Err(format!("Clear DTCs NRC 0x{:02X}: {}", nrc, nrc_text(nrc)));
    }
    if !validate_checksum(frame) {
        return Err("Clear DTC response checksum mismatch".to_string());
    }
    if frame.len() < 4 {
        return Err("Clear DTC response too short".to_string());
    }
    if frame[3] != 0x44 {
        return Err(format!("Clear DTC: unexpected SID 0x{:02X}", frame[3]));
    }
    Ok(())
}

/// Parse Mode 02 freeze frame response for a specific PID.
/// Returns the raw data bytes (caller decodes).
fn parse_mode02_response(frame: &[u8], pid: u8) -> Result<Vec<u8>, String> {
    if frame.len() >= 6 && frame[3] == 0x7F {
        return Err(format!("Freeze frame NRC 0x{:02X}", frame[5]));
    }
    if !validate_checksum(frame) {
        return Err("Freeze frame checksum mismatch".to_string());
    }
    if frame.len() < 7 {
        return Err(format!("Freeze frame response too short: {} bytes", frame.len()));
    }
    // frame[3]=0x42, frame[4]=PID, frame[5]=FF#, data starts at frame[6]
    if frame[3] != 0x42 || frame[4] != pid {
        return Err(format!("Freeze frame: unexpected SID/PID 0x{:02X}/0x{:02X}", frame[3], frame[4]));
    }
    Ok(frame[6..frame.len() - 1].to_vec())
}

fn nrc_text(nrc: u8) -> &'static str {
    match nrc {
        0x11 => "service not supported",
        0x12 => "sub-function not supported",
        0x22 => "conditions not correct",
        0x31 => "request out of range",
        0x33 => "security access denied",
        0x78 => "response pending",
        _    => "unknown NRC",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DTC reader — reads all three mode groups
// ─────────────────────────────────────────────────────────────────────────────

/// Read all DTCs from the ECM:
///   - Stored  (Mode 03 → response SID 0x43)
///   - Pending (Mode 07 → response SID 0x47)
///   - Permanent (Mode 0A → response SID 0x4A, NRC 0x11 = not supported → empty)
///
/// Requires: Security Level 1 active (Mode 03/07/0A are read-only,
///           but the P01 requires at least L1 for diagnostic services).
pub fn read_dtcs(port: &mut Box<dyn SerialPort>) -> Result<DtcReadResult, String> {
    // Stored DTCs — Mode 03
    let stored = read_dtc_group(port, 0x03, 0x43, false, false)?;

    // Pending DTCs — Mode 07
    let pending = read_dtc_group(port, 0x07, 0x47, true, false)?;

    // Permanent DTCs — Mode 0A (may not be supported on all P01 variants)
    let permanent = read_dtc_group(port, 0x0A, 0x4A, false, true)?;

    let total = stored.len() + pending.len() + permanent.len();
    Ok(DtcReadResult { stored, pending, permanent, total })
}

/// Internal: send a DTC request, collect all response frames, decode DTCs.
fn read_dtc_group(
    port:          &mut Box<dyn SerialPort>,
    req_sid:       u8,
    resp_sid:      u8,
    is_pending:    bool,
    is_permanent:  bool,
) -> Result<Vec<DtcRecord>, String> {
    write_frame(port, &build_request(req_sid))?;

    // Collect frames — P01 may split >6 DTCs across multiple responses.
    // Read until timeout (read_response returns Err on timeout).
    let mut all_pairs: Vec<[u8; 2]> = Vec::new();
    loop {
        match read_response(port) {
            Ok(frame) => {
                if frame.is_empty() { break; }
                match parse_dtc_response(&frame, resp_sid) {
                    Ok(pairs) => all_pairs.extend_from_slice(&pairs),
                    Err(_)    => break,  // Unexpected frame (e.g. another node) — stop
                }
            }
            Err(_) => break,  // Timeout — no more frames
        }
    }

    let records = all_pairs
        .iter()
        .filter_map(|&[b1, b2]| make_record(b1, b2, is_pending, is_permanent))
        .collect();
    Ok(records)
}

// ─────────────────────────────────────────────────────────────────────────────
// DTC clear — Mode 04
// ─────────────────────────────────────────────────────────────────────────────

/// Clear all DTCs and reset readiness monitors (Mode 04).
///
/// ⚠️  This clears ALL stored, pending, and freeze frame data.
/// ⚠️  Readiness monitors are reset — the vehicle will need a full drive cycle
///    before emissions monitors are ready again.
/// ⚠️  Requires: Security Level 1 (Mode 04 does NOT require L2 on the P01;
///    it is a standard OBD-II service accessible without programming access).
pub fn clear_dtcs(
    port:          &mut Box<dyn SerialPort>,
    prior_count:   usize,
) -> Result<DtcClearResult, String> {
    write_frame(port, &build_request(0x04))?;
    let resp = read_response(port)?;

    match parse_clear_response(&resp) {
        Ok(()) => Ok(DtcClearResult {
            success:       true,
            cleared_count: prior_count,
            message:       format!(
                "DTCs cleared successfully. {} code(s) removed. \
                 Readiness monitors reset — complete a full drive cycle before emissions test.",
                prior_count
            ),
        }),
        Err(e) => Ok(DtcClearResult {
            success:       false,
            cleared_count: 0,
            message:       e,
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Freeze frame — Mode 02
// ─────────────────────────────────────────────────────────────────────────────

/// Read freeze frame data for the first stored DTC.
///
/// Polls the following Mode 02 PIDs:
///   0x02 — DTC that triggered the freeze frame
///   0x04 — Calculated engine load (%)
///   0x05 — Engine coolant temperature (°C)
///   0x0B — Intake manifold absolute pressure (kPa)
///   0x0C — Engine RPM
///   0x0D — Vehicle speed (km/h)
///   0x0E — Ignition advance (degrees)
///   0x0F — Intake air temperature (°C)
///   0x10 — MAF air flow rate (g/s)
///   0x11 — Throttle position (%)
///   0x06 — Short-term fuel trim bank 1 (%)
///   0x07 — Long-term fuel trim bank 1 (%)
pub fn read_freeze_frame(port: &mut Box<dyn SerialPort>) -> Result<FreezeFrameResult, String> {
    let mut ff = FreezeFrameResult {
        trigger_dtc:   None,
        engine_load:   None,
        ect_c:         None,
        map_kpa:       None,
        rpm:           None,
        vss_kph:       None,
        spark_adv_deg: None,
        iat_c:         None,
        maf_gs:        None,
        tps_pct:       None,
        stft_b1_pct:   None,
        ltft_b1_pct:   None,
    };

    // PID 0x02 — DTC that triggered freeze frame
    if let Ok(data) = poll_ff_pid(port, 0x02) {
        if data.len() >= 2 {
            ff.trigger_dtc = decode_dtc_bytes(data[0], data[1]);
        }
    }

    // Engine load %
    if let Ok(data) = poll_ff_pid(port, 0x04) {
        if let Some(&b) = data.first() {
            ff.engine_load = Some(b as f32 * 100.0 / 255.0);
        }
    }
    // ECT °C
    if let Ok(data) = poll_ff_pid(port, 0x05) {
        if let Some(&b) = data.first() {
            ff.ect_c = Some(b as f32 - 40.0);
        }
    }
    // MAP kPa
    if let Ok(data) = poll_ff_pid(port, 0x0B) {
        if let Some(&b) = data.first() {
            ff.map_kpa = Some(b as f32);
        }
    }
    // RPM
    if let Ok(data) = poll_ff_pid(port, 0x0C) {
        if data.len() >= 2 {
            ff.rpm = Some(((data[0] as f32 * 256.0) + data[1] as f32) / 4.0);
        }
    }
    // VSS km/h
    if let Ok(data) = poll_ff_pid(port, 0x0D) {
        if let Some(&b) = data.first() {
            ff.vss_kph = Some(b as f32);
        }
    }
    // Spark advance degrees
    if let Ok(data) = poll_ff_pid(port, 0x0E) {
        if let Some(&b) = data.first() {
            ff.spark_adv_deg = Some(b as f32 / 2.0 - 64.0);
        }
    }
    // IAT °C
    if let Ok(data) = poll_ff_pid(port, 0x0F) {
        if let Some(&b) = data.first() {
            ff.iat_c = Some(b as f32 - 40.0);
        }
    }
    // MAF g/s
    if let Ok(data) = poll_ff_pid(port, 0x10) {
        if data.len() >= 2 {
            ff.maf_gs = Some(((data[0] as f32 * 256.0) + data[1] as f32) / 100.0);
        }
    }
    // TPS %
    if let Ok(data) = poll_ff_pid(port, 0x11) {
        if let Some(&b) = data.first() {
            ff.tps_pct = Some(b as f32 * 100.0 / 255.0);
        }
    }
    // STFT B1 %
    if let Ok(data) = poll_ff_pid(port, 0x06) {
        if let Some(&b) = data.first() {
            ff.stft_b1_pct = Some(b as f32 * 100.0 / 128.0 - 100.0);
        }
    }
    // LTFT B1 %
    if let Ok(data) = poll_ff_pid(port, 0x07) {
        if let Some(&b) = data.first() {
            ff.ltft_b1_pct = Some(b as f32 * 100.0 / 128.0 - 100.0);
        }
    }

    Ok(ff)
}

fn poll_ff_pid(port: &mut Box<dyn SerialPort>, pid: u8) -> Result<Vec<u8>, String> {
    write_frame(port, &build_mode02_request(pid))?;
    let resp = read_response(port)?;
    parse_mode02_response(&resp, pid)
}

// ─────────────────────────────────────────────────────────────────────────────
// DTC description table — P0xxx powertrain codes common on the LS1 P01
// ─────────────────────────────────────────────────────────────────────────────

/// Return a short description for known P01/LS1 DTC codes.
/// Returns a generic fallback for unknown codes.
pub fn describe_dtc(code: &str) -> String {
    match code {
        // ── Misfire ──────────────────────────────────────────────────────────
        "P0300" => "Random/multiple cylinder misfire detected",
        "P0301" => "Cylinder 1 misfire detected",
        "P0302" => "Cylinder 2 misfire detected",
        "P0303" => "Cylinder 3 misfire detected",
        "P0304" => "Cylinder 4 misfire detected",
        "P0305" => "Cylinder 5 misfire detected",
        "P0306" => "Cylinder 6 misfire detected",
        "P0307" => "Cylinder 7 misfire detected",
        "P0308" => "Cylinder 8 misfire detected",
        // ── Fuel system ──────────────────────────────────────────────────────
        "P0171" => "System too lean (Bank 1)",
        "P0172" => "System too rich (Bank 1)",
        "P0174" => "System too lean (Bank 2)",
        "P0175" => "System too rich (Bank 2)",
        "P0201" => "Injector circuit open — Cylinder 1",
        "P0202" => "Injector circuit open — Cylinder 2",
        "P0203" => "Injector circuit open — Cylinder 3",
        "P0204" => "Injector circuit open — Cylinder 4",
        "P0205" => "Injector circuit open — Cylinder 5",
        "P0206" => "Injector circuit open — Cylinder 6",
        "P0207" => "Injector circuit open — Cylinder 7",
        "P0208" => "Injector circuit open — Cylinder 8",
        // ── MAF / MAP / sensors ───────────────────────────────────────────────
        "P0100" => "MAF circuit malfunction",
        "P0101" => "MAF circuit range/performance",
        "P0102" => "MAF circuit low input",
        "P0103" => "MAF circuit high input",
        "P0105" => "MAP circuit malfunction",
        "P0106" => "MAP circuit range/performance",
        "P0107" => "MAP circuit low input",
        "P0108" => "MAP circuit high input",
        // ── Coolant / temp ────────────────────────────────────────────────────
        "P0115" => "Engine coolant temperature circuit malfunction",
        "P0116" => "Engine coolant temperature circuit range/performance",
        "P0117" => "Engine coolant temperature circuit low input",
        "P0118" => "Engine coolant temperature circuit high input",
        // ── TPS ───────────────────────────────────────────────────────────────
        "P0120" => "Throttle position sensor A circuit malfunction",
        "P0121" => "Throttle position sensor A circuit range/performance",
        "P0122" => "Throttle position sensor A circuit low input",
        "P0123" => "Throttle position sensor A circuit high input",
        // ── Oxygen sensors ────────────────────────────────────────────────────
        "P0130" => "O2 sensor circuit malfunction (Bank 1 Sensor 1)",
        "P0131" => "O2 sensor circuit low voltage (Bank 1 Sensor 1)",
        "P0132" => "O2 sensor circuit high voltage (Bank 1 Sensor 1)",
        "P0133" => "O2 sensor circuit slow response (Bank 1 Sensor 1)",
        "P0134" => "O2 sensor circuit no activity (Bank 1 Sensor 1)",
        "P0136" => "O2 sensor circuit malfunction (Bank 1 Sensor 2)",
        "P0150" => "O2 sensor circuit malfunction (Bank 2 Sensor 1)",
        "P0151" => "O2 sensor circuit low voltage (Bank 2 Sensor 1)",
        "P0152" => "O2 sensor circuit high voltage (Bank 2 Sensor 1)",
        "P0154" => "O2 sensor circuit no activity (Bank 2 Sensor 1)",
        "P0156" => "O2 sensor circuit malfunction (Bank 2 Sensor 2)",
        // ── IAT ───────────────────────────────────────────────────────────────
        "P0110" => "Intake air temperature circuit malfunction",
        "P0112" => "Intake air temperature circuit low input",
        "P0113" => "Intake air temperature circuit high input",
        // ── VSS ───────────────────────────────────────────────────────────────
        "P0500" => "Vehicle speed sensor malfunction",
        "P0502" => "Vehicle speed sensor circuit low input",
        "P0503" => "Vehicle speed sensor circuit high/erratic/intermittent",
        // ── Knock ─────────────────────────────────────────────────────────────
        "P0325" => "Knock sensor 1 circuit malfunction (Bank 1)",
        "P0326" => "Knock sensor 1 circuit range/performance (Bank 1)",
        "P0327" => "Knock sensor 1 circuit low input (Bank 1)",
        "P0328" => "Knock sensor 1 circuit high input (Bank 1)",
        "P0330" => "Knock sensor 2 circuit malfunction (Bank 2)",
        // ── Crank / cam ───────────────────────────────────────────────────────
        "P0335" => "Crankshaft position sensor A circuit malfunction",
        "P0336" => "Crankshaft position sensor A circuit range/performance",
        "P0340" => "Camshaft position sensor A circuit malfunction (Bank 1)",
        "P0341" => "Camshaft position sensor A circuit range/performance",
        // ── Cooling fan ───────────────────────────────────────────────────────
        "P0480" => "Cooling fan 1 control circuit malfunction",
        "P0481" => "Cooling fan 2 control circuit malfunction",
        // ── EVAP ──────────────────────────────────────────────────────────────
        "P0440" => "Evaporative emission control system malfunction",
        "P0441" => "Evaporative emission control system incorrect purge flow",
        "P0442" => "Evaporative emission control system leak detected (small)",
        "P0446" => "Evaporative emission control system vent control circuit",
        "P0449" => "Evaporative emission vent valve solenoid circuit malfunction",
        // ── Transmission (RE4 / 4L60E) ────────────────────────────────────────
        "P0700" => "Transmission control system malfunction",
        "P0711" => "Transmission fluid temperature sensor circuit range/performance",
        "P0712" => "Transmission fluid temperature sensor circuit low input",
        "P0713" => "Transmission fluid temperature sensor circuit high input",
        "P0720" => "Output speed sensor circuit malfunction",
        "P0722" => "Output speed sensor circuit no signal",
        "P0730" => "Incorrect gear ratio",
        "P0740" => "Torque converter clutch circuit malfunction",
        "P0748" => "Pressure control solenoid A electrical",
        "P0751" => "Shift solenoid A performance or stuck off",
        "P0752" => "Shift solenoid A stuck on",
        "P0756" => "Shift solenoid B performance or stuck off",
        "P0757" => "Shift solenoid B stuck on",
        // ── GM P01-specific ───────────────────────────────────────────────────
        "P1133" => "HO2S insufficient switching (Bank 1 Sensor 1)",
        "P1134" => "HO2S transition time ratio (Bank 1 Sensor 1)",
        "P1153" => "HO2S insufficient switching (Bank 2 Sensor 1)",
        "P1154" => "HO2S transition time ratio (Bank 2 Sensor 1)",
        "P1345" => "CMP-CKP correlation",
        "P1380" => "Misfire detected — rough road data not available",
        "P1381" => "Misfire detected — no communication with brake control module",
        "P1415" => "Secondary air injection (AIR) system (Bank 1)",
        "P1416" => "Secondary air injection (AIR) system (Bank 2)",
        "P1441" => "EVAP system flow during non-purge",
        "P1514" => "TAC system — performance (high airflow)",
        "P1515" => "TAC module — commanded and actual throttle positions",
        "P1516" => "TAC module — throttle actuator control range/performance",
        "P1517" => "TAC module — throttle actuator control performance",
        "P1518" => "TAC module — serial communication problem",
        "P1520" => "Park/Neutral position switch circuit",
        "P1621" => "PCM memory performance",
        "P1626" => "Theft deterrent fuel enable signal lost",
        "P1629" => "Theft deterrent fuel enable signal not received",
        "P1635" => "5 volt reference 1 circuit",
        "P1639" => "5 volt reference 2 circuit",
        "P1810" => "TFP valve position switch circuit",
        "P1860" => "TCC PWM solenoid circuit electrical",
        "P1887" => "TCC release switch circuit",
        _ => "No description available",
    }.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DTC byte decoder ─────────────────────────────────────────────────────

    #[test]
    fn decode_p0300_random_misfire() {
        // P0300 = 0x03 0x00
        assert_eq!(decode_dtc_bytes(0x03, 0x00), Some("P0300".to_string()));
    }

    #[test]
    fn decode_p0171_lean_b1() {
        // P0171: prefix=P(00), d1=0(00), d2=1, d3=7, d4=1 → 0x01 0x71
        assert_eq!(decode_dtc_bytes(0x01, 0x71), Some("P0171".to_string()));
    }

    #[test]
    fn decode_zero_bytes_is_none() {
        assert_eq!(decode_dtc_bytes(0x00, 0x00), None);
    }

    #[test]
    fn decode_chassis_dtc() {
        // C prefix: bits 7-6 = 01 → b1 high nibble with 01 prefix
        // C0040 = 0x40 0x40
        // b1=0x40: (0x40>>6)&0x03 = 1 → C, (0x40>>4)&0x03 = 0, 0x40&0x0F = 0
        // b2=0x40: (0x40>>4)&0x0F = 4, 0x40&0x0F = 0 → C0040
        assert_eq!(decode_dtc_bytes(0x40, 0x40), Some("C0040".to_string()));
    }

    #[test]
    fn decode_body_dtc() {
        // B prefix: bits 7-6 = 10
        // B0100 = 0x81 0x00
        // b1=0x81: (0x81>>6)&0x03 = 2 → B, (0x81>>4)&0x03=0, 0x81&0x0F=1
        // b2=0x00: d3=0, d4=0 → B0100
        assert_eq!(decode_dtc_bytes(0x81, 0x00), Some("B0100".to_string()));
    }

    #[test]
    fn decode_network_dtc() {
        // U prefix: bits 7-6 = 11
        // U0100 = 0xC1 0x00
        // b1=0xC1: (0xC1>>6)&0x03 = 3 → U, (0xC1>>4)&0x03 = 0, 0xC1&0x0F = 1
        // b2=0x00 → U0100
        assert_eq!(decode_dtc_bytes(0xC1, 0x00), Some("U0100".to_string()));
    }

    #[test]
    fn decode_p1345_cmp_ckp() {
        // P1345 = 0x13 0x45
        // b1=0x13: prefix=P(00), d1=(0x13>>4)&0x03=1, d2=0x13&0x0F=3
        // b2=0x45: d3=(0x45>>4)&0x0F=4, d4=0x45&0x0F=5 → P1345
        assert_eq!(decode_dtc_bytes(0x13, 0x45), Some("P1345".to_string()));
    }

    // ── Frame builder checksum ────────────────────────────────────────────────

    fn checksum_ok(frame: &[u8]) -> bool {
        if frame.len() < 2 { return false; }
        frame[..frame.len()-1].iter().fold(0u8, |a, &b| a.wrapping_add(b))
            == frame[frame.len()-1]
    }

    #[test]
    fn mode03_frame_checksum() {
        let f = build_request(0x03);
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x03);
    }

    #[test]
    fn mode04_frame_checksum() {
        let f = build_request(0x04);
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x04);
    }

    #[test]
    fn mode07_frame_checksum() {
        let f = build_request(0x07);
        assert!(checksum_ok(&f));
        assert_eq!(f[3], 0x07);
    }

    // ── Response parser ───────────────────────────────────────────────────────

    #[test]
    fn parse_mode03_response_two_dtcs() {
        // 48 6B 10  43  02  03 00  01 71  <cs>
        // SID=0x43, count=2, DTC1=0x03,0x00(P0300), DTC2=0x01,0x71(P0171)
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x43, 0x02, 0x03, 0x00, 0x01, 0x71];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        let pairs = parse_dtc_response(&frame, 0x43).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(decode_dtc_bytes(pairs[0][0], pairs[0][1]), Some("P0300".to_string()));
        assert_eq!(decode_dtc_bytes(pairs[1][0], pairs[1][1]), Some("P0171".to_string()));
    }

    #[test]
    fn parse_mode03_response_no_dtcs() {
        // 48 6B 10  43  00  <cs> — count=0
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x43, 0x00];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        let pairs = parse_dtc_response(&frame, 0x43).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn parse_mode04_clear_ack() {
        // 48 6B 10  44  <cs>
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x44];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        assert!(parse_clear_response(&frame).is_ok());
    }

    #[test]
    fn nrc_0x11_treated_as_empty_not_error() {
        // 48 6B 10  7F  0A  11  <cs> — NRC 0x11 for Mode 0A = not supported
        let mut frame = vec![0x48u8, 0x6B, 0x10, 0x7F, 0x0A, 0x11];
        let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame.push(cs);
        let result = parse_dtc_response(&frame, 0x4A);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
