//! Protocol-aware OBD transport.
//!
//! Cheap ELM327 adapters speak ASCII (`010C\r` → `41 0C ..`).
//! Raw VPW adapters want a J1850 header + checksum.
//! OBDLink / STN devices support the full ELM AT set plus the richer ST command set
//! (see OBDLink Family Reference and Programming Manual).
//!
//! Sending VPW bytes down an ELM port is why live data looked dead.

use serialport::SerialPort;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::elm_parse::{parse_reply, ElmError, ElmReply, HeaderMode};

use crate::pid_decode::*;
use crate::vpw::{
    ascii_from_obd_payload, build_mode09_request, build_obd_request, parse_mode01_response,
    parse_mode09_response, request_response,
};

/// Protocol of the current adapter session. Read from the transport's own
/// session cache, NOT from `crate::STATE`: callers run inside `with_port`, which
/// already holds the STATE lock, and re-locking a std Mutex on the same thread
/// deadlocks (this hung every live read before v3.45.0).
pub fn current_protocol() -> String {
    with_session(|s| s.protocol.clone())
}

pub fn is_elm_ascii(protocol: &str) -> bool {
    let p = protocol.to_ascii_lowercase();
    p.contains("elm")
        || p.contains("auto")
        || p.contains("can")
        || p.contains("uds")
        || p.contains("kwp")
        || p.contains("consult")
        || p.contains("obdlink")
        || p.contains("stn")
        || p.is_empty()
}

pub fn is_raw_vpw(protocol: &str) -> bool {
    let p = protocol.to_ascii_lowercase();
    p.contains("vpw") || p.contains("j1850")
}

/// Map a user-facing protocol name to an STP number (FRPM).
/// 12 = J1850 VPW, 33 = ISO 15765 11-bit 500 kbps, 5 = ISO 14230 KWP fast, etc.
pub fn protocol_to_stp(protocol: &str) -> Option<&'static str> {
    let p = protocol.to_ascii_lowercase();
    if p.contains("vpw") || p.contains("j1850") {
        Some("12")
    } else if p.contains("uds") || p.contains("can") || p.contains("iso15765") {
        Some("33")
    } else if p.contains("kwp") {
        Some("5")
    } else if p.contains("iso9141") || p.contains("9141") {
        Some("3")
    } else {
        None
    }
}

/// Read window for one request once the protocol is known. Matches `ATST96`
/// (0x96 x 4 ms = 600 ms adapter timeout) plus margin for the prompt.
pub const READ_WINDOW_MS: u64 = 1000;
/// First OBD request of a session: the adapter may run a protocol search
/// (`SEARCHING...`), which takes several seconds on ELM327/STN devices.
pub const SEARCH_WINDOW_MS: u64 = 6000;
/// P2* window: keep listening after `7F xx 78` (response pending), no new request.
pub const PENDING_EXTRA_MS: u64 = 5000;
/// Consecutive NO DATA replies before logging stops ("ignition off?").
/// The reference log shows 41 in a row with the ignition off.
pub const NO_DATA_STOP_THRESHOLD: u32 = 10;

/// Read until the `>` prompt or the deadline. A serial read timeout is not the
/// end of the reply: the adapter can be silent for seconds during a protocol
/// search, so only the prompt or the deadline stops the read.
fn read_ascii(port: &mut Box<dyn SerialPort + Send>, wait_ms: u64) -> String {
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    let mut collected = String::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                collected.push_str(&String::from_utf8_lossy(&buf[..n]));
                if collected.contains('>') {
                    break;
                }
            }
            Ok(_) => std::thread::sleep(Duration::from_millis(8)),
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                std::thread::sleep(Duration::from_millis(8));
            }
            Err(_) => break,
        }
    }
    collected
}

fn reply_is_error(raw: &str) -> bool {
    let up = raw.to_ascii_uppercase();
    up.contains("UNABLE")
        || up.contains("STOPPED")
        || up.contains("ERROR")
        || up.contains("BUFFER FULL")
        || up.replace(' ', "").contains("NODATA")
        || up.split(|c| c == '\r' || c == '\n').any(|l| l.replace('>', "").trim() == "?")
}

/// Send an AT or ST command and wait for the prompt.
/// Returns the raw response. Errors on UNABLE / ERROR / STOPPED / BUFFER FULL /
/// NO DATA / `?` (unknown command): none of those are success.
pub fn elm_command(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    elm_command_timeout(port, cmd, READ_WINDOW_MS)
}

pub fn elm_command_timeout(port: &mut Box<dyn SerialPort + Send>, cmd: &str, wait_ms: u64) -> Result<String, String> {
    let raw = elm_send_raw(port, cmd, wait_ms)?;
    if reply_is_error(&raw) {
        return Err(format!("ELM/ST {cmd}: {}", raw.replace('>', "").trim()));
    }
    Ok(raw)
}

fn elm_send_raw(port: &mut Box<dyn SerialPort + Send>, cmd: &str, wait_ms: u64) -> Result<String, String> {
    let _ = port.clear(serialport::ClearBuffer::Input);
    port.write_all(format!("{}\r", cmd).as_bytes())
        .map_err(|e| format!("ELM/ST write {cmd}: {e}"))?;
    port.flush().ok();
    Ok(read_ascii(port, wait_ms))
}

/// Send an OBD/UDS request (hex text) and parse the reply line by line.
///
/// Response pending: while any ECU has sent `7F <sid> 78` without its final
/// answer, keep listening — also after the `>` prompt — for up to the P2*
/// window (`sess.p2_star_ms`, 5 s) WITHOUT sending anything (a bare CR would
/// make the ELM repeat the last request). If the window ends with an ECU still
/// pending, that (SID, ECU) is remembered: the next request with the same SID
/// treats that ECU's answer as ambiguous (could be the late answer) and discards
/// it. Answers are always matched by SID echo + PID/DID prefix (`positive`), so a
/// stray frame for another service is never taken as the answer.
pub fn elm_request(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
    cmd: &str,
    mode: HeaderMode,
    wait_ms: u64,
) -> Result<ElmReply, ElmError> {
    let mut raw = elm_send_raw(port, cmd, wait_ms).map_err(ElmError::Io)?;
    let sid = u8::from_str_radix(cmd.get(..2).unwrap_or(""), 16).unwrap_or(0);
    let mut reply = parse_reply(&raw, mode);
    let deadline = Instant::now() + Duration::from_millis(sess.p2_star_ms);
    while !reply.pending_headers(sid).is_empty() && Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u64;
        let more = read_ascii(port, left.min(250).max(1));
        if !more.is_empty() {
            raw.push('\r');
            raw.push_str(&more);
            reply = parse_reply(&raw, mode);
        }
    }
    if let Some((psid, ph)) = sess.unresolved_pending.take() {
        if psid == sid {
            reply.ambiguous.push((psid, ph));
        } else {
            // Different service: keep remembering until a same-SID request comes.
            sess.unresolved_pending = Some((psid, ph));
        }
    }
    if let Some(h) = reply.pending_headers(sid).into_iter().next() {
        sess.unresolved_pending = Some((sid, h));
    }
    Ok(reply)
}

/// Commands that put an ELM327/STN back to the app's CAN text defaults:
/// CAN auto-formatting on, CAN receive filters/masks reset (ATCRA per the
/// ELM327 datasheet; ATAR automatic receive), flow-control mode default,
/// STN filters cleared, functional OBD header, headers on, spaces off, ATST96.
/// Unknown commands on a given adapter just answer `?` and are ignored.
pub const CAN_TEXT_DEFAULTS: &[&str] = &[
    "ATCAF1", "STCAF1", "ATCRA", "ATAR", "ATFCSM0", "STFAC", "ATSH7DF", "ATH1", "ATS0", "ATST96",
];

/// Drop guard: owns the port borrow while code changes adapter state (CAF,
/// CM/CF filters, headers, diagnostic session) and sends `restore` commands when
/// it goes out of scope, on success, `?` early return, error or panic alike.
pub struct AdapterGuard<'a> {
    port: &'a mut Box<dyn SerialPort + Send>,
    restore: Vec<String>,
}

impl<'a> AdapterGuard<'a> {
    pub fn new(port: &'a mut Box<dyn SerialPort + Send>, restore: &[&str]) -> Self {
        mark_text_mode_dirty();
        Self { port, restore: restore.iter().map(|s| s.to_string()).collect() }
    }
    pub fn port(&mut self) -> &mut Box<dyn SerialPort + Send> {
        self.port
    }
    /// Drop one restore command (it was already done explicitly and confirmed).
    pub fn done(&mut self, cmd: &str) {
        self.restore.retain(|c| c != cmd);
    }
}

impl Drop for AdapterGuard<'_> {
    fn drop(&mut self) {
        for c in &self.restore {
            let _ = elm_command_timeout(self.port, c, 400);
        }
        mark_text_mode_dirty();
    }
}

/// Send a best-effort setup command; a `?` (or other error) is not fatal but is
/// logged in the session, e.g. bare STCAF1 / STFAC / ATCRA, which the MX+ in the
/// Patrol log was never seen answering (documented in the ELM327/STN manuals).
fn best_effort(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession, cmd: &str, wait_ms: u64) {
    if let Err(e) = elm_command_timeout(port, cmd, wait_ms) {
        let note = format!("{}: {} (ignored)", cmd, e.rsplit(": ").next().unwrap_or(&e));
        if sess.recovery_log.len() >= 64 {
            sess.recovery_log.remove(0);
        }
        sess.recovery_log.push(note);
    }
}

/// Re-initialise CAN text formatting after foreign frames showed up. The
/// caller's header is put back if it was the Nissan ECM header (7E0).
pub fn recover_formatting(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) {
    for c in CAN_TEXT_DEFAULTS {
        best_effort(port, sess, c, 400);
    }
    if sess.header_7e0 {
        best_effort(port, sess, "ATSH7E0", 400);
    }
    TEXT_MODE_DIRTY.store(false, Ordering::SeqCst);
    sess.text_mode_ready = true;
    sess.recoveries = sess.recoveries.saturating_add(1);
}

/// Last resort when foreign frames survive the re-init: ATZ and the full warmup
/// (echo/linefeeds/spaces off, headers on, protocol), then the CAN text defaults.
pub fn full_reinit(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) {
    let _ = elm_send_raw(port, "ATZ", 1500);
    let mut warm = vec!["ATE0", "ATL0", "ATS0", "ATH1"];
    let p = sess.protocol.to_ascii_lowercase();
    if p.contains("can") || p.contains("uds") {
        warm.push("ATSP6");
    }
    for c in warm {
        best_effort(port, sess, c, 400);
    }
    recover_formatting(port, sess);
    sess.full_reinits = sess.full_reinits.saturating_add(1);
}

/// Request with a foreign-frame check: if the reply contains CAN frames from IDs
/// that are not OBD responders (7E8..7EF / 18DAF1xx), the adapter's filters or
/// formatting are dirty (e.g. left on STCAF0 + ATCM000). Re-init the defaults; use
/// the answer if a valid one was in the reply, otherwise retry. If the flood
/// persists, ATZ + full warmup and one last retry.
pub fn request_checked(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
    cmd: &str,
    wait_ms: u64,
    sid: u8,
    prefix: &[u8],
) -> Result<(ElmReply, Vec<u8>), ElmError> {
    let mode = header_mode_for(&sess.protocol);
    let reply = elm_request(port, sess, cmd, mode, wait_ms)?;
    if reply.foreign_headers().is_empty() {
        let d = reply.positive(sid, prefix)?;
        return Ok((reply, d));
    }
    recover_formatting(port, sess);
    if let Ok(d) = reply.positive(sid, prefix) {
        return Ok((reply, d));
    }
    let mut retry = elm_request(port, sess, cmd, mode, wait_ms)?;
    if !retry.foreign_headers().is_empty() {
        if let Ok(d) = retry.positive(sid, prefix) {
            return Ok((retry, d));
        }
        full_reinit(port, sess);
        retry = elm_request(port, sess, cmd, mode, wait_ms)?;
        if !retry.foreign_headers().is_empty() {
            let d = retry.positive(sid, prefix).map_err(|e| ElmError::Adapter(format!(
                "foreign CAN frames ({}) still present after re-init and ATZ: {}", retry.foreign_headers().join(","), e)))?;
            return Ok((retry, d));
        }
    }
    let d = retry.positive(sid, prefix)?;
    Ok((retry, d))
}

// ── Session cache ──────────────────────────────────────────────────────────

/// Per-connection ELM/STN state. Reset on connect/disconnect.
#[derive(Debug, Clone)]
pub struct ElmSession {
    pub protocol: String,
    /// Mode 01 PIDs the ECU reported as supported (from 0100/0120/0140...).
    pub supported: Option<BTreeSet<u8>>,
    /// A first OBD request succeeded, so the adapter's protocol search is done.
    pub protocol_found: bool,
    /// Replies carried CAN headers (3 or 8 digits): response-count suffix is safe.
    pub can_frames: bool,
    pub no_data_streak: u32,
    /// ATCAF1 ATH1 ATS0 ATST96 sent for this session.
    pub text_mode_ready: bool,
    /// P2* window to keep listening after `7F xx 78` (ms). 5000 by default.
    pub p2_star_ms: u64,
    /// (SID, ECU header) left pending when the last P2* window ran out.
    pub unresolved_pending: Option<(u8, String)>,
    /// Times the adapter formatting was re-initialised after foreign frames.
    pub recoveries: u32,
    /// Times the re-init was not enough and ATZ + full warmup was done.
    pub full_reinits: u32,
    /// Setup commands that answered `?` or an error (non-fatal), most recent last.
    pub recovery_log: Vec<String>,
    /// Adapter header currently set to the Nissan ECM (ATSH7E0).
    pub header_7e0: bool,
    /// Nissan 10C0 session open for a raw-DID poll block.
    pub nissan_open: bool,
    /// Samples polled in the current Nissan block.
    pub nissan_block_samples: u32,
    /// Times 10C0 was re-sent mid-block after the ECU dropped the session.
    pub nissan_reentries: u32,
}

impl ElmSession {
    pub const fn new() -> Self {
        Self {
            protocol: String::new(), supported: None, protocol_found: false, can_frames: false,
            no_data_streak: 0, text_mode_ready: false, p2_star_ms: PENDING_EXTRA_MS,
            unresolved_pending: None, recoveries: 0, full_reinits: 0, recovery_log: Vec::new(),
            header_7e0: false, nissan_open: false, nissan_block_samples: 0, nissan_reentries: 0,
        }
    }
    pub fn for_protocol(protocol: &str) -> Self {
        let mut s = Self::new();
        s.protocol = protocol.to_string();
        s
    }
}

impl Default for ElmSession {
    fn default() -> Self { Self::new() }
}

static ELM_SESSION: Mutex<ElmSession> = Mutex::new(ElmSession::new());
/// Set by code that switches the adapter to raw CAN framing (ATCAF0 / ATH0),
/// so the next OBD request restores text formatting first.
static TEXT_MODE_DIRTY: AtomicBool = AtomicBool::new(false);

pub fn mark_text_mode_dirty() {
    TEXT_MODE_DIRTY.store(true, Ordering::SeqCst);
}

/// Start a new adapter session (call from connect/auto-detect).
pub fn begin_session(protocol: &str) {
    if let Ok(mut g) = ELM_SESSION.lock() {
        *g = ElmSession::for_protocol(protocol);
    }
}

pub fn end_session() {
    begin_session("");
}

/// Run `f` with the session cache locked. Never call this from inside another
/// `with_session` closure.
pub fn with_session<R>(f: impl FnOnce(&mut ElmSession) -> R) -> R {
    let mut g = ELM_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

pub fn session_snapshot() -> serde_json::Value {
    with_session(|s| serde_json::json!({
        "protocol": s.protocol,
        "supported_pids": s.supported.as_ref().map(|set| set.iter().map(|p| format!("{:02X}", p)).collect::<Vec<_>>()),
        "protocol_found": s.protocol_found,
        "can_frames": s.can_frames,
        "no_data_streak": s.no_data_streak,
        "formatting_recoveries": s.recoveries,
        "full_reinits": s.full_reinits,
        "recovery_log": s.recovery_log,
        "nissan_block_open": s.nissan_open,
        "nissan_session_reentries": s.nissan_reentries,
        "unresolved_pending": s.unresolved_pending.as_ref().map(|(sid, h)| format!("{} SID {:02X}", h, sid)),
    }))
}

/// Header layout to expect for the session protocol (headers are on: `ATH1`).
pub fn header_mode_for(protocol: &str) -> HeaderMode {
    let p = protocol.to_ascii_lowercase();
    if p.contains("can") || p.contains("uds") || p.contains("iso15765") {
        HeaderMode::Can11
    } else if p.contains("vpw") || p.contains("j1850") || p.contains("kwp") || p.contains("9141") {
        HeaderMode::ThreeByte
    } else {
        HeaderMode::Auto
    }
}

/// Make sure the adapter prints formatted frames with headers (ATCAF1 ATH1 ATS0)
/// and uses the 600 ms response timeout (ATST96) before an OBD text request.
pub fn ensure_text_mode(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession) {
    if TEXT_MODE_DIRTY.swap(false, Ordering::SeqCst) {
        sess.text_mode_ready = false;
    }
    if sess.text_mode_ready {
        return;
    }
    for c in ["ATCAF1", "ATH1", "ATS0", "ATST96"] {
        let _ = elm_command(port, c);
    }
    sess.text_mode_ready = true;
}

/// Where a supply-voltage reading came from. Labels are shown in the UI and the
/// audit log, so they must name the real source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoltageSource {
    /// OBDLink `STVR` (adapter pin 16 voltage, calibrated).
    AdapterStvr,
    /// ELM327 `ATRV` (adapter pin 16 voltage).
    AdapterAtrv,
    /// SAE Mode 01 PID 0x42 (control module voltage) via the ELM text path.
    Pid42,
    /// SAE Mode 01 PID 0x42 over the raw VPW frame path.
    Pid42VpwRaw,
}

impl VoltageSource {
    pub fn label(self) -> &'static str {
        match self {
            VoltageSource::AdapterStvr => "adapter_stvr",
            VoltageSource::AdapterAtrv => "adapter_atrv",
            VoltageSource::Pid42 => "pid42",
            VoltageSource::Pid42VpwRaw => "pid42_vpw_raw",
        }
    }
}

/// Adapter voltage: OBDLink STVR first, then ELM ATRV. Returns the source.
pub fn read_adapter_voltage_src(port: &mut Box<dyn SerialPort + Send>) -> Option<(f32, VoltageSource)> {
    // STVR is the recommended command per FRPM (ATRV is deprecated on STN).
    if let Ok(raw) = elm_command(port, "STVR") {
        if let Some(v) = crate::voltage_watch::parse_at_rv(&raw) {
            return Some((v, VoltageSource::AdapterStvr));
        }
    }
    // Fallback for pure ELM327 clones.
    if let Ok(raw) = elm_command(port, "ATRV") {
        if let Some(v) = crate::voltage_watch::parse_at_rv(&raw) {
            return Some((v, VoltageSource::AdapterAtrv));
        }
    }
    None
}

/// Preferred adapter voltage read (OBDLink STVR). Falls back to ATRV.
pub fn read_adapter_voltage(port: &mut Box<dyn SerialPort + Send>) -> Option<f32> {
    read_adapter_voltage_src(port).map(|(v, _)| v)
}

/// Supply voltage with its real source: adapter STVR/ATRV first; on the ELM text
/// path, Mode 01 PID 0x42 only when the ECU lists 0x42 as supported.
pub fn voltage_with_source_in(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
) -> Option<(f32, VoltageSource)> {
    if let Some(r) = read_adapter_voltage_src(port) {
        return Some(r);
    }
    if !is_elm_path(&sess.protocol) {
        return None;
    }
    let supported = read_support_in(port, sess).ok()?;
    if !supported.contains(&0x42) {
        return None;
    }
    let d = mode01_in(port, sess, 0x42).ok()?;
    crate::pid_decode::decode_module_voltage_obd(&d).map(|v| (v, VoltageSource::Pid42))
}

/// Session-locking wrapper for `voltage_with_source_in`.
pub fn read_voltage_with_source(port: &mut Box<dyn SerialPort + Send>) -> Option<(f32, VoltageSource)> {
    with_session(|sess| voltage_with_source_in(port, sess))
}

/// Identify the adapter (STDI / STI). Useful for confirming MX+ and logging firmware.
pub fn identify_adapter(port: &mut Box<dyn SerialPort + Send>) -> String {
    let mut id = String::new();
    if let Ok(hw) = elm_command(port, "STDI") {
        id.push_str(hw.trim());
    }
    if let Ok(fw) = elm_command(port, "STI") {
        if !id.is_empty() {
            id.push(' ');
        }
        id.push_str(fw.trim());
    }
    if id.is_empty() {
        if let Ok(ati) = elm_command(port, "ATI") {
            id = ati.trim().to_string();
        }
    }
    id
}

/// Set protocol via STP (preferred over ATSP).
/// Examples: STP 12 for J1850 VPW, STP 33 for ISO 15765 11-bit 500 kbps.
pub fn set_protocol(port: &mut Box<dyn SerialPort + Send>, stp: &str) -> Result<String, String> {
    elm_command(port, &format!("STP {}", stp))
}

/// Open the currently selected protocol (STPO).
pub fn open_protocol(port: &mut Box<dyn SerialPort + Send>) -> Result<String, String> {
    elm_command(port, "STPO")
}

/// Set request timeout in ms (STPTO).
pub fn set_timeout_ms(port: &mut Box<dyn SerialPort + Send>, ms: u32) -> Result<String, String> {
    elm_command(port, &format!("STPTO {}", ms))
}

/// Clear all filters (STFAC / ATCRA for non-CAN).
pub fn clear_filters(port: &mut Box<dyn SerialPort + Send>) -> Result<String, String> {
    // STFAC is the modern clear; fall back to ATCRA if needed.
    elm_command(port, "STFAC").or_else(|_| elm_command(port, "ATCRA"))
}

/// Set a CAN flow-control filter (STCFCPA) or generic pass filter (STFAP).
/// `id` is the header/ID to accept (hex, no spaces).
pub fn set_pass_filter(port: &mut Box<dyn SerialPort + Send>, id: &str) -> Result<String, String> {
    elm_command(port, &format!("STFAP {}", id))
}

/// Set a CAN flow-control pass filter (STCFCPA).
/// `id` is the CAN ID the adapter should accept for ISO-TP flow control (hex, no spaces).
pub fn set_flow_control_filter(port: &mut Box<dyn SerialPort + Send>, id: &str) -> Result<String, String> {
    elm_command(port, &format!("STCFCPA {}", id))
}

/// Start bus monitoring (STM). Returns the first chunk of traffic or the prompt.
pub fn start_monitor(port: &mut Box<dyn SerialPort + Send>) -> Result<String, String> {
    elm_command(port, "STM")
}

/// Stop monitoring (any key / ATZ-style abort). Sends a newline to return to prompt.
pub fn stop_monitor(port: &mut Box<dyn SerialPort + Send>) -> Result<String, String> {
    let _ = port.write_all(b"\r");
    port.flush().ok();
    Ok(read_ascii(port, 200))
}

/// Capture a short bus dump via STM for `ms` milliseconds, then stop.
/// Useful for seeing traffic without a long-running monitor.
pub fn capture_monitor(port: &mut Box<dyn SerialPort + Send>, ms: u64) -> Result<String, String> {
    let _ = port.clear(serialport::ClearBuffer::Input);
    port.write_all(b"STM\r").map_err(|e| format!("STM write: {e}"))?;
    port.flush().ok();
    let dump = read_ascii(port, ms.max(100));
    // Abort and return to prompt.
    let _ = port.write_all(b"\r");
    port.flush().ok();
    let _ = read_ascii(port, 150);
    Ok(dump)
}

/// Enter PowerSave / sleep (STSLEEP). Adapter wakes on UART activity or bus activity
/// depending on STSLPAS configuration.
pub fn enter_sleep(port: &mut Box<dyn SerialPort + Send>) -> Result<String, String> {
    elm_command(port, "STSLEEP")
}

/// Configure PowerSave wake sources (STSLPAS).
/// Typical values: 1 = UART, 2 = bus, 3 = both (device-dependent; see FRPM).
pub fn set_sleep_wake(port: &mut Box<dyn SerialPort + Send>, mode: u8) -> Result<String, String> {
    elm_command(port, &format!("STSLPAS {}", mode))
}

/// Read a programmable parameter (ATPP). Returns the raw response.
pub fn read_pp(port: &mut Box<dyn SerialPort + Send>, pp: u8) -> Result<String, String> {
    elm_command(port, &format!("ATPP {:02X} OFF", pp))
}

/// Write a programmable parameter (ATPP xx SV yy ON). Persistent across power cycles.
pub fn write_pp(port: &mut Box<dyn SerialPort + Send>, pp: u8, value: u8) -> Result<String, String> {
    elm_command(port, &format!("ATPP {:02X} SV {:02X} ON", pp, value))
}

/// Send an arbitrary message via STPX (up to 4 KB on MX+).
/// `data` is the raw payload bytes (header + SID + data). Adapter adds protocol framing.
/// Returns the parsed response bytes.
pub fn stpx(port: &mut Box<dyn SerialPort + Send>, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Err("STPX: empty payload".into());
    }
    let hex: String = data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("");
    let cmd = format!("STPX {}", hex);
    let reply = with_session(|sess| elm_request(port, sess, &cmd, HeaderMode::Auto, READ_WINDOW_MS)).map_err(|e| e.to_string())?;
    if let Some(m) = reply.messages.first() {
        return Ok(m.payload.clone());
    }
    if let Some(e) = reply.adapter_errors.first() {
        return Err(format!("STPX: {}", e));
    }
    Err(if reply.no_data { "STPX: NO DATA".into() } else { "STPX: no frame in reply".into() })
}

/// Basic MX+ / OBDLink setup for a known protocol.
/// Sets protocol, opens it, and reads adapter voltage. Logs identity.
pub fn setup_obdlink(port: &mut Box<dyn SerialPort + Send>, stp: &str) -> Result<f32, String> {
    let id = identify_adapter(port);
    if !id.is_empty() {
        eprintln!("OBDLink: {}", id);
    }
    set_protocol(port, stp)?;
    open_protocol(port)?;
    // 600 ms, matching ATST96 (what Car Scanner ran error-free on the Patrol).
    // The old 100 ms was shorter than the ECU's normal reply time on some buses.
    let _ = set_timeout_ms(port, 600);
    read_adapter_voltage(port).ok_or_else(|| "No STVR/ATRV after setup".to_string())
}

/// Legacy: join every hex digit and cut pairs. Only valid for header-less text
/// such as hex typed by the user. On adapter replies with `ATH1` the 3-digit CAN
/// header shifts every byte, so replies go through `elm_parse::parse_reply`.
pub fn parse_elm_hex(raw: &str) -> Vec<u8> {
    crate::elm_parse::hex_from_text(raw)
}

fn is_elm_path(protocol: &str) -> bool {
    !(is_raw_vpw(protocol) && !is_elm_ascii(protocol))
}

/// One Mode 01 request with session bookkeeping: protocol-search timeout on the
/// first request, response-count suffix once CAN is confirmed, NO DATA streak.
pub fn mode01_in(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
    pid: u8,
) -> Result<Vec<u8>, ElmError> {
    if !is_elm_path(&sess.protocol) {
        let r = request_response(port, &build_obd_request(pid))
            .map_err(ElmError::Io)
            .and_then(|resp| parse_mode01_response(&resp, pid).ok_or_else(|| ElmError::Unmatched("VPW frame".into())));
        match &r {
            Ok(_) => sess.no_data_streak = 0,
            Err(_) => sess.no_data_streak = sess.no_data_streak.saturating_add(1),
        }
        return r;
    }
    ensure_text_mode(port, sess);
    let mut cmd = format!("01{:02X}", pid);
    if sess.protocol_found && sess.can_frames {
        // ELM327 >= v1.3: trailing digit = number of responses to wait for.
        cmd.push('1');
    }
    let wait = if sess.protocol_found { READ_WINDOW_MS } else { SEARCH_WINDOW_MS };
    match request_checked(port, sess, &cmd, wait, 0x01, &[pid]) {
        Ok((reply, d)) => {
            sess.no_data_streak = 0;
            sess.protocol_found = true;
            if reply.messages.iter().any(|m| matches!(m.header.as_deref().map(str::len), Some(3) | Some(8))) {
                sess.can_frames = true;
            }
            Ok(d)
        }
        Err(e) => {
            if e == ElmError::NoData {
                sess.no_data_streak = sess.no_data_streak.saturating_add(1);
            }
            Err(e)
        }
    }
}

/// Read (and cache) the Mode 01 support masks: 0100, then 0120/0140/... while
/// the previous mask advertises the next one. Fails when 0100 is not answered.
pub fn read_support_in(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
) -> Result<BTreeSet<u8>, ElmError> {
    if let Some(s) = &sess.supported {
        return Ok(s.clone());
    }
    let mut set = BTreeSet::new();
    let mut base: u8 = 0x00;
    loop {
        let mask = mode01_in(port, sess, base)?;
        if mask.len() < 4 {
            return Err(ElmError::Unmatched(format!("01{:02X} mask too short", base)));
        }
        let pids = crate::elm_parse::decode_support_mask(base as u16, &mask[..4]);
        let next = base.checked_add(0x20);
        let has_next = next.map(|n| pids.contains(&(n as u16))).unwrap_or(false);
        for p in pids {
            if p % 0x20 != 0 && p <= 0xFF {
                set.insert(p as u8);
            }
        }
        match next {
            Some(n) if has_next && n < 0xE0 => base = n,
            _ => break,
        }
    }
    sess.supported = Some(set.clone());
    Ok(set)
}

/// Pull Mode 01 PID payload using whichever transport the session opened.
pub fn pull_mode01(port: &mut Box<dyn SerialPort + Send>, pid: u8) -> Option<Vec<u8>> {
    with_session(|sess| mode01_in(port, sess, pid).ok())
}

pub fn mode09_in(port: &mut Box<dyn SerialPort + Send>, sess: &mut ElmSession, info: u8) -> Result<String, ElmError> {
    if !is_elm_path(&sess.protocol) {
        return request_response(port, &build_mode09_request(info))
            .map_err(ElmError::Io)
            .and_then(|resp| parse_mode09_response(&resp, info).ok_or_else(|| ElmError::Unmatched("VPW frame".into())))
            .map(|d| ascii_from_obd_payload(&d))
            .and_then(|s| if s.is_empty() { Err(ElmError::Unmatched("empty".into())) } else { Ok(s) });
    }
    ensure_text_mode(port, sess);
    let wait = if sess.protocol_found { READ_WINDOW_MS * 2 } else { SEARCH_WINDOW_MS };
    let (_, data) = request_checked(port, sess, &format!("09{:02X}", info), wait, 0x09, &[info])?;
    let s = ascii_from_obd_payload(&data);
    if s.is_empty() {
        // e.g. the Patrol answers 0902 with an all-zero VIN.
        return Err(ElmError::Unmatched("blank (all-zero) value".into()));
    }
    Ok(s)
}

pub fn pull_mode09(port: &mut Box<dyn SerialPort + Send>, info: u8) -> Option<String> {
    with_session(|sess| mode09_in(port, sess, info).ok())
}

pub fn elm_service(port: &mut Box<dyn SerialPort + Send>, sid: u8) -> Result<Vec<u8>, String> {
    with_session(|sess| {
        let mode = header_mode_for(&sess.protocol);
        let reply = elm_request(port, sess, &format!("{:02X}", sid), mode, READ_WINDOW_MS).map_err(|e| e.to_string())?;
        reply.positive(sid, &[]).map_err(|e| e.to_string())
    })
}

// ── Live channels ──────────────────────────────────────────────────────────

fn first_byte(d: &[u8]) -> Option<f32> { d.first().map(|&b| b as f32) }

/// SAE J1979 Mode 01 channels the app can poll. `batt` is handled separately
/// (adapter voltage first). Only PIDs the ECU lists as supported are polled.
pub const MODE01_CHANNELS: &[(&str, u8, fn(&[u8]) -> Option<f32>)] = &[
    ("rpm", 0x0C, decode_engine_rpm),
    ("map", 0x0B, decode_map),
    ("ect", 0x05, decode_ect),
    ("tps", 0x11, decode_throttle_pos),
    ("iat", 0x0F, decode_iat),
    ("spark", 0x0E, decode_timing_advance),
    ("stft", 0x06, decode_stft_bank1),
    ("ltft", 0x07, decode_ltft_bank1),
    ("maf", 0x10, decode_maf_obd),
    ("vss", 0x0D, decode_vss),
    ("load", 0x04, decode_engine_load),
    ("o2b1s1", 0x14, decode_o2_b1s1_obd),
    ("o2b1s2", 0x15, decode_o2_b1s2_obd),
    ("baro", 0x33, first_byte),
    ("fuel_status", 0x03, decode_fuel_system_status),
    ("fuel_level", 0x2F, decode_fuel_level),
    ("rail", 0x23, decode_fuel_rail_gauge_pressure),
    ("dist_mil", 0x21, decode_distance_with_mil),
];

pub fn mode01_pid_for(id: &str) -> Option<u8> {
    MODE01_CHANNELS.iter().find(|c| c.0 == id).map(|c| c.1)
}

#[derive(Debug, Clone, Default)]
pub struct LivePoll {
    pub values: BTreeMap<String, f64>,
    /// Requested channels whose PID the ECU does not list as supported (not polled).
    pub unsupported: Vec<String>,
    /// Supported channels that were polled but failed (NO DATA, negative, ...).
    pub failed: Vec<String>,
    pub voltage_source: Option<&'static str>,
    pub no_data_streak: u32,
    pub supported: Vec<u8>,
}

impl LivePoll {
    pub fn should_stop(&self) -> bool {
        self.no_data_streak >= NO_DATA_STOP_THRESHOLD
    }
}

pub fn ignition_off_message(streak: u32) -> String {
    format!("{} consecutive NO DATA replies: ignition off? Logging stopped.", streak)
}

/// Poll the requested channel ids, skipping PIDs the ECU does not support.
/// Errors when the support mask (0100) cannot be read.
pub fn poll_live_in(
    port: &mut Box<dyn SerialPort + Send>,
    sess: &mut ElmSession,
    wanted: &[String],
) -> Result<LivePoll, String> {
    let mut out = LivePoll::default();
    let supported = match read_support_in(port, sess) {
        Ok(s) => s,
        Err(e) => {
            out.no_data_streak = sess.no_data_streak;
            return Err(if sess.no_data_streak >= NO_DATA_STOP_THRESHOLD {
                ignition_off_message(sess.no_data_streak)
            } else {
                format!("PID support mask (0100) not answered: {}. Check ignition and protocol.", e)
            });
        }
    };
    out.supported = supported.iter().copied().collect();
    let nissan: Vec<u16> = wanted.iter().filter_map(|id| nissan_raw_channel_did(id)).collect();
    if sess.header_7e0 && wanted.iter().any(|id| nissan_raw_channel_did(id).is_none()) {
        // Mode 01 goes functional (7DF); the Nissan block keeps its session open.
        if elm_command(port, "ATSH7DF").is_ok() {
            sess.header_7e0 = false;
        }
    }
    for id in wanted.iter().filter(|id| nissan_raw_channel_did(id).is_none()) {
        if id == "batt" {
            if let Some((v, src)) = voltage_with_source_in(port, sess) {
                out.values.insert("batt".into(), v as f64);
                out.voltage_source = Some(src.label());
            } else {
                out.failed.push("batt: no STVR/ATRV and PID 0x42 not supported".into());
            }
            continue;
        }
        let Some(&(_, pid, decode)) = MODE01_CHANNELS.iter().find(|c| c.0 == id.as_str()) else {
            out.unsupported.push(id.clone());
            continue;
        };
        if !supported.contains(&pid) {
            out.unsupported.push(id.clone());
            continue;
        }
        match mode01_in(port, sess, pid) {
            Ok(d) => match decode(&d) {
                Some(v) => { out.values.insert(id.clone(), v as f64); }
                None => out.failed.push(format!("{}: undecodable {:02X?}", id, d)),
            },
            Err(e) => out.failed.push(format!("{}: {}", id, e)),
        }
        if sess.no_data_streak >= NO_DATA_STOP_THRESHOLD {
            break;
        }
    }
    if !nissan.is_empty() && sess.no_data_streak < NO_DATA_STOP_THRESHOLD {
        match crate::consult::nissan_poll_raw_in(port, sess, &nissan) {
            Ok(vals) => {
                for (did, v) in vals {
                    match v {
                        Ok(x) => { out.values.insert(format!("nissan_{:04X}", did), x as f64); }
                        Err(e) => out.failed.push(format!("nissan_{:04X}: {}", did, e)),
                    }
                }
            }
            Err(e) => out.failed.extend(nissan.iter().map(|d| format!("nissan_{:04X}: {}", d, e))),
        }
    }
    out.no_data_streak = sess.no_data_streak;
    Ok(out)
}

/// Raw Nissan DID logging channel id: `nissan_1101` -> 0x1101 (meaning unknown).
pub fn nissan_raw_channel_did(id: &str) -> Option<u16> {
    let h = id.strip_prefix("nissan_")?;
    let d = u16::from_str_radix(h, 16).ok()?;
    crate::consult::PATROL_ZD30_SUPPORTED_IDS.contains(&d).then_some(d)
}

pub fn poll_live(port: &mut Box<dyn SerialPort + Send>, wanted: &[String]) -> Result<LivePoll, String> {
    with_session(|sess| poll_live_in(port, sess, wanted))
}

pub fn all_channel_ids() -> Vec<String> {
    let mut v: Vec<String> = MODE01_CHANNELS.iter().map(|c| c.0.to_string()).collect();
    v.push("batt".into());
    v
}

pub fn live_map_from_poll(poll: Result<LivePoll, String>, protocol: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    match poll {
        Ok(p) => {
            for (k, v) in &p.values {
                obj.insert(k.clone(), serde_json::json!(v));
            }
            obj.insert("pids_decoded".into(), serde_json::json!(p.values.len()));
            obj.insert("source".into(), serde_json::json!(if p.values.is_empty() { "live-empty" } else { "live-Mode01" }));
            obj.insert("unsupported".into(), serde_json::json!(p.unsupported));
            obj.insert("failed".into(), serde_json::json!(p.failed));
            obj.insert("voltage_source".into(), serde_json::json!(p.voltage_source));
            obj.insert("supported_pids".into(), serde_json::json!(p.supported.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>()));
        }
        Err(e) => {
            obj.insert("pids_decoded".into(), serde_json::json!(0));
            obj.insert("source".into(), serde_json::json!("live-empty"));
            obj.insert("error".into(), serde_json::json!(e));
        }
    }
    obj.insert("honest".into(), serde_json::json!(true));
    obj.insert("transport".into(), serde_json::json!(protocol));
    obj
}

pub fn decode_live_map(port: &mut Box<dyn SerialPort + Send>) -> serde_json::Map<String, serde_json::Value> {
    let ids = all_channel_ids();
    let proto = current_protocol();
    live_map_from_poll(poll_live(port, &ids), &proto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_port::{ReplayPort, SentLog};

    const FIXTURE: &str = include_str!("../../reference/fixtures/elm_patrol_zd30_obdlink.txt");

    const CLEANUP: &str = include_str!("../../reference/fixtures/elm_cleanup_and_pending.txt");

    /// Raw Section S4 transcripts: `>CMD\rREPLY...\r\r>CMD2...` -> [(cmd, reply_with_prompt)].
    fn s4_case(n: usize) -> Vec<(String, String)> {
        let raw = CLEANUP.lines().filter(|l| l.contains("raw: >19")).nth(n).unwrap();
        let raw = raw.split_once("raw: ").unwrap().1.replace("\\r", "\r").replace("\\n", "\n");
        raw.split('>')
            .filter_map(|seg| {
                let (cmd, rest) = seg.split_once('\r')?;
                (cmd.len() >= 4 && cmd.chars().all(|c| c.is_ascii_hexdigit())).then(|| (cmd.to_string(), format!("{}>", rest)))
            })
            .collect()
    }

    fn s4_port(n: usize) -> (Box<dyn SerialPort + Send>, SentLog) {
        let mut p = ReplayPort::empty();
        for (cmd, reply) in s4_case(n) {
            p = p.with_reply(&cmd, &[reply.as_str()]);
        }
        p.boxed()
    }

    fn quick_sess() -> ElmSession {
        let mut s = ElmSession::for_protocol("can");
        s.p2_star_ms = 300;
        s
    }

    #[test]
    fn s4_pending_ecu_answer_in_next_reply_is_ambiguous() {
        // 24.08 case 1: 7E8 pending for 190208, prompt came; next 1902AC reply starts 7E8 59 02 FF.
        let (mut port, log) = s4_port(0);
        let mut sess = quick_sess();
        let r = elm_request(&mut port, &mut sess, "190208", HeaderMode::Can11, 200).unwrap();
        // Kept listening through P2* without sending anything else.
        assert_eq!(log.get(), vec!["190208"]);
        assert_eq!(r.positive_from(Some("7E8"), 0x19, &[0x02]), Err(ElmError::Pending { sid: 0x19 }));
        assert_eq!(r.positive_from(Some("7E9"), 0x19, &[0x02]).unwrap(), vec![0xFF]);
        assert_eq!(sess.unresolved_pending, Some((0x19, "7E8".to_string())));
        let r = elm_request(&mut port, &mut sess, "1902AC", HeaderMode::Can11, 200).unwrap();
        // 7E8's 59 02 FF may be the late answer to 190208: never used for 1902AC.
        assert_eq!(r.positive_from(Some("7E8"), 0x19, &[0x02]), Err(ElmError::Ambiguous { sid: 0x19, header: "7E8".into() }));
        assert_eq!(r.positive_from(Some("7E9"), 0x19, &[0x02]).unwrap(), vec![0xFF, 0x17, 0x08, 0x00, 0x20]);
        assert_eq!(r.positive_from(Some("7EB"), 0x19, &[0x02]).unwrap().len(), 13);
        assert_eq!(sess.unresolved_pending, None);
    }

    #[test]
    fn s4_still_pending_in_next_reply() {
        // 24.08 case 3: 190208 pending on 7E8; 1902AC gets 7E8 7F 19 78 again.
        let (mut port, _log) = s4_port(2);
        let mut sess = quick_sess();
        let _ = elm_request(&mut port, &mut sess, "190208", HeaderMode::Can11, 200).unwrap();
        let r = elm_request(&mut port, &mut sess, "1902AC", HeaderMode::Can11, 200).unwrap();
        assert!(r.positive_from(Some("7E8"), 0x19, &[0x02]).is_err());
        assert_eq!(sess.unresolved_pending, Some((0x19, "7E8".to_string())));
        // 1902AF: 7E8 59 02 FF could answer 1902AC -> ambiguous, discarded.
        let r = elm_request(&mut port, &mut sess, "1902AF", HeaderMode::Can11, 200).unwrap();
        assert!(matches!(r.positive_from(Some("7E8"), 0x19, &[0x02]), Err(ElmError::Ambiguous { .. })));
    }

    const SESSION_ID: &str = include_str!("../../reference/fixtures/elm_patrol_session_identity.txt");

    #[test]
    fn real_24aug_late_answer_in_next_reply_is_ambiguous() {
        // Real 24 Aug sequence, 7E8 frames only (Data Logger item 5):
        // >190208\r7E8037F1978\r\r>1902AC\r7E8035902FF\r\r>
        let raw = SESSION_ID.lines().find(|l| l.contains("raw (7E9/7EB lines removed)")).unwrap();
        let raw = raw.split_once(": ").unwrap().1.replace("\\r", "\r");
        assert_eq!(raw, ">190208\r7E8037F1978\r\r>1902AC\r7E8035902FF\r\r>");
        let mut p = ReplayPort::empty();
        for seg in raw.split('>').filter(|s| !s.is_empty()) {
            let (cmd, rest) = seg.split_once('\r').unwrap();
            p = p.with_reply(cmd, &[format!("{}>", rest).as_str()]);
        }
        let (mut port, log) = p.boxed();
        let mut sess = quick_sess();
        let r = elm_request(&mut port, &mut sess, "190208", HeaderMode::Can11, 200).unwrap();
        assert_eq!(r.positive_from(Some("7E8"), 0x19, &[0x02]), Err(ElmError::Pending { sid: 0x19 }));
        assert_eq!(log.get(), vec!["190208"], "nothing sent during P2*");
        let r = elm_request(&mut port, &mut sess, "1902AC", HeaderMode::Can11, 200).unwrap();
        assert_eq!(r.positive_from(Some("7E8"), 0x19, &[0x02]), Err(ElmError::Ambiguous { sid: 0x19, header: "7E8".into() }));
        assert_eq!(r.positive(0x19, &[0x02]), Err(ElmError::Ambiguous { sid: 0x19, header: "7E8".into() }));
    }

    #[test]
    fn synthetic_late_answer_after_prompt_is_picked_up_without_new_request() {
        // SYNTHETIC: the Data Logger found no case in any session of a final answer
        // arriving on its own after the '>'. Shape only; bytes reuse 7E8 03 59 02 FF.
        let (mut port, log) = ReplayPort::empty().with_reply("190208", &["7E8037F1978\r\r>7E8035902FF\r\r>"]).boxed();
        let mut sess = quick_sess();
        let r = elm_request(&mut port, &mut sess, "190208", HeaderMode::Can11, 200).unwrap();
        assert_eq!(r.positive_from(Some("7E8"), 0x19, &[0x02]).unwrap(), vec![0xFF]);
        assert_eq!(log.get(), vec!["190208"]);
        assert_eq!(sess.unresolved_pending, None);
    }

    #[test]
    fn patrol_0902_pending_answered_before_prompt() {
        let line = CLEANUP.lines().find(|l| l.starts_with("> 0902")).unwrap();
        let resp = line.split_once("< ").unwrap().1.replace(" | ", "\r");
        let (mut port, _log) = ReplayPort::empty().with_reply("0902", &[resp.as_str()]).boxed();
        let mut sess = quick_sess();
        let r = elm_request(&mut port, &mut sess, "0902", HeaderMode::Can11, 200).unwrap();
        assert_eq!(r.positive(0x09, &[0x02]).unwrap().len(), 18);
        assert_eq!(sess.unresolved_pending, None);
    }

    #[test]
    fn stray_dtc_frame_is_not_a_mode01_answer() {
        let (mut port, _log) = ReplayPort::empty().with_reply("0100", &["7E8035902FF"]).boxed();
        let mut sess = quick_sess();
        assert!(matches!(mode01_in(&mut port, &mut sess, 0x00), Err(ElmError::Unmatched(_))));
    }

    #[test]
    fn s3_foreign_frames_trigger_reinit_and_are_never_decoded() {
        let (mut port, log) = ReplayPort::from_fixture(CLEANUP, "## Section S3a").boxed();
        let mut sess = quick_sess();
        let mut ok = 0;
        let mut err = 0;
        for _ in 0..30 {
            match mode01_in(&mut port, &mut sess, 0x00) {
                Ok(d) => { assert_eq!(d, vec![0x98, 0x39, 0x00, 0x11]); ok += 1; }
                Err(_) => err += 1,
            }
        }
        assert!(ok > 0 && err > 0, "ok {ok} err {err}");
        assert!(sess.recoveries > 0);
        let sent = log.get();
        for c in ["ATCAF1", "ATCRA", "ATAR", "ATSH7DF"] {
            assert!(sent.iter().any(|s| s == c), "{c} not sent");
        }
    }

    #[test]
    fn reinit_question_mark_is_logged_not_fatal() {
        // Bare STCAF1 / STFAC / ATCRA were never seen answered on the MX+; here they answer '?'.
        let mut p = ReplayPort::empty().with_reply("0100", &["2D1200000030E", "7E806410098390011"]);
        for c in CAN_TEXT_DEFAULTS.iter().filter(|c| !matches!(**c, "STCAF1" | "STFAC" | "ATCRA")) {
            p = p.with_reply(c, &["OK"]);
        }
        let (mut port, _log) = p.boxed();
        let mut sess = quick_sess();
        assert_eq!(mode01_in(&mut port, &mut sess, 0x00).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
        assert_eq!(sess.recovery_log.len(), 3, "{:?}", sess.recovery_log);
        for c in ["STCAF1", "STFAC", "ATCRA"] {
            assert!(sess.recovery_log.iter().any(|l| l.starts_with(c) && l.contains('?') && l.ends_with("(ignored)")), "{:?}", sess.recovery_log);
        }
        assert!(!sess.recovery_log.iter().any(|l| l.starts_with("ATFCSM0")));
        assert_eq!(sess.full_reinits, 0);
    }

    #[test]
    fn persistent_flood_falls_back_to_atz_and_warmup() {
        let (mut port, log) = ReplayPort::empty()
            .with_reply("0100", &["2D1200000030E", "23D23000000007DFE8A", "7E806410098390011"])
            .with_reply("ATZ", &["ELM327 v1.4b"])
            .boxed();
        let mut sess = quick_sess();
        sess.protocol = "can".into();
        assert_eq!(mode01_in(&mut port, &mut sess, 0x00).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
        assert_eq!(sess.full_reinits, 1);
        let sent = log.get();
        let z = sent.iter().position(|c| c == "ATZ").unwrap();
        for c in ["ATE0", "ATL0", "ATS0", "ATH1", "ATSP6", "ATCAF1", "ATSH7DF"] {
            assert!(sent[z..].iter().any(|s| s == c), "{c} after ATZ");
        }
        assert_eq!(sent.last().map(String::as_str), Some("0100"));
    }

    #[test]
    fn s3_clean_reply_after_reinit_is_used() {
        let (mut port, log) = ReplayPort::empty().with_reply("0100", &["2D1200000030E", "7E806410098390011"]).boxed();
        let mut sess = quick_sess();
        assert_eq!(mode01_in(&mut port, &mut sess, 0x00).unwrap(), vec![0x98, 0x39, 0x00, 0x11]);
        let sent = log.get();
        let first = sent.iter().position(|c| c == "0100").unwrap();
        let reinit = sent.iter().position(|c| c == "ATCRA").unwrap();
        assert!(first < reinit && sent.iter().rposition(|c| c == "0100").unwrap() > reinit);
        assert_eq!(sess.recoveries, 1);
    }

    #[test]
    fn s3_no_data_run_stops_logging() {
        let (mut port, _log) = ReplayPort::from_fixture(CLEANUP, "## Section S3b").boxed();
        let mut sess = quick_sess();
        let mut last = String::new();
        for _ in 0..NO_DATA_STOP_THRESHOLD {
            last = poll_live_in(&mut port, &mut sess, &["rpm".to_string()]).unwrap_err();
        }
        assert!(last.contains("ignition off"), "{last}");
    }

    #[test]
    fn poll_live_handles_nissan_raw_channels() {
        // A1 has the 10C0 / mask exchanges but no value reads: the raw channel fails, nothing is invented.
        let (mut port, log) = patrol_port();
        let mut sess = quick_sess();
        let p = poll_live_in(&mut port, &mut sess, &["rpm".into(), "nissan_1101".into()]).unwrap();
        assert!(p.values.contains_key("rpm"));
        assert!(!p.values.contains_key("nissan_1101"));
        assert!(p.failed.iter().any(|f| f.starts_with("nissan_1101")));
        // Block stays open across samples; closing sends 1081(1) + ATSH7DF.
        assert!(sess.nissan_open);
        assert!(log.get().iter().any(|c| c == "10C01"), "suffix once CAN is known");
        crate::consult::nissan_block_end_in(&mut port, &mut sess);
        let sent = log.get();
        assert_eq!(&sent[sent.len() - 2..], &["10811", "ATSH7DF"]);
    }

    fn patrol_port() -> (Box<dyn SerialPort + Send>, SentLog) {
        ReplayPort::from_fixture(FIXTURE, "## Section A1").with_reply("ATRV", &["13.8V"]).boxed()
    }

    #[test]
    fn legacy_join_parser_shifts_bytes_with_headers() {
        // Why replies no longer go through parse_elm_hex: 7E8 header shifts every byte.
        let old = parse_elm_hex("7E804410C0BBC");
        assert_eq!(old, vec![0x7E, 0x80, 0x44, 0x10, 0xC0, 0xBB]);
        assert!(!old.windows(2).any(|w| w == [0x41, 0x0C]));
        assert_eq!(parse_elm_hex("41 0C 1A F8\r>"), vec![0x41, 0x0C, 0x1A, 0xF8]);
    }

    #[test]
    fn current_protocol_does_not_relock_state() {
        // Holding STATE (as with_port does) must not deadlock current_protocol().
        let _g = crate::STATE.lock().unwrap_or_else(|e| e.into_inner());
        let _ = current_protocol();
    }

    #[test]
    fn protocol_classifiers() {
        assert!(is_elm_ascii("can"));
        assert!(is_elm_ascii("uds"));
        assert!(is_elm_ascii("elm-auto (Mode 01 seen)"));
        assert!(is_elm_ascii("obdlink"));
        assert!(is_raw_vpw("vpw"));
        assert!(!is_raw_vpw("can"));
    }

    #[test]
    fn protocol_to_stp_maps() {
        assert_eq!(protocol_to_stp("vpw"), Some("12"));
        assert_eq!(protocol_to_stp("j1850"), Some("12"));
        assert_eq!(protocol_to_stp("can"), Some("33"));
        assert_eq!(protocol_to_stp("uds"), Some("33"));
        assert_eq!(protocol_to_stp("kwp"), Some("5"));
        assert_eq!(protocol_to_stp("unknown"), None);
    }

    #[test]
    fn replay_support_masks_match_patrol() {
        let (mut port, log) = patrol_port();
        let mut sess = ElmSession::for_protocol("can");
        let s = read_support_in(&mut port, &mut sess).unwrap();
        let v: Vec<u8> = s.into_iter().collect();
        assert_eq!(v, vec![0x01, 0x04, 0x05, 0x0B, 0x0C, 0x0D, 0x10, 0x1C, 0x21, 0x23]);
        assert!(sess.protocol_found && sess.can_frames);
        // Cached: no second 0100 sent.
        let before = log.get().len();
        read_support_in(&mut port, &mut sess).unwrap();
        assert_eq!(log.get().len(), before);
    }

    #[test]
    fn replay_poll_only_supported_pids() {
        let (mut port, log) = patrol_port();
        let mut sess = ElmSession::for_protocol("can");
        let p = poll_live_in(&mut port, &mut sess, &all_channel_ids()).unwrap();
        // Real replies to the count-suffixed requests: 010C1 -> 0BB8 = 750 rpm,
        // 0105 -> 50 = 40 C, 010D1 -> 0 km/h, 01231 -> 0D64 = 34280 kPa, 010B1 -> 63 = 99 kPa.
        assert_eq!(p.values.get("rpm"), Some(&750.0));
        assert_eq!(p.values.get("ect"), Some(&40.0));
        assert_eq!(p.values.get("vss"), Some(&0.0));
        assert_eq!(p.values.get("rail"), Some(&34280.0));
        assert_eq!(p.values.get("map"), Some(&99.0));
        assert!(p.values.contains_key("maf"));
        for id in ["tps", "iat", "spark", "stft", "ltft", "o2b1s1", "o2b1s2", "baro", "fuel_status", "fuel_level"] {
            assert!(p.unsupported.iter().any(|u| u == id), "{id} should be skipped");
        }
        let sent = log.get();
        for never in ["0111", "010F", "010E", "0106", "0107", "0114", "0115", "0133", "0103", "012F", "0142"] {
            assert!(!sent.iter().any(|c| c.starts_with(never)), "polled unsupported {never}");
        }
        // Response-count suffix once CAN is confirmed.
        assert!(sent.iter().any(|c| c == "010C1"), "{:?}", sent);
        assert_eq!(p.voltage_source, Some("adapter_atrv"));
        assert_eq!(p.values.get("batt"), Some(&(13.8f32 as f64)));
    }

    #[test]
    fn replay_text_mode_setup_sent() {
        let (mut port, log) = patrol_port();
        let mut sess = ElmSession::for_protocol("can");
        let _ = mode01_in(&mut port, &mut sess, 0x0C);
        let sent = log.get();
        for c in ["ATCAF1", "ATH1", "ATS0", "ATST96"] {
            assert!(sent.iter().any(|s| s == c), "{c} missing: {:?}", sent);
        }
    }

    #[test]
    fn no_data_streak_stops() {
        let (mut port, _log) = ReplayPort::empty().boxed();
        let mut sess = ElmSession::for_protocol("can");
        let mut last = Err(String::new());
        for _ in 0..NO_DATA_STOP_THRESHOLD {
            last = poll_live_in(&mut port, &mut sess, &["rpm".to_string()]);
        }
        let e = last.unwrap_err();
        assert!(e.contains("ignition off"), "{e}");
        assert!(sess.no_data_streak >= NO_DATA_STOP_THRESHOLD);
    }

    #[test]
    fn replay_voltage_prefers_adapter_and_labels_source() {
        let (mut port, _log) = ReplayPort::empty().with_reply("STVR", &["12.6"]).boxed();
        let mut sess = ElmSession::for_protocol("can");
        assert_eq!(voltage_with_source_in(&mut port, &mut sess), Some((12.6, VoltageSource::AdapterStvr)));
        // No adapter voltage, ECU without PID 0x42 (Patrol): no reading, and 0142 never sent.
        let (mut port, log) = patrol_port_no_atrv();
        let mut sess = ElmSession::for_protocol("can");
        assert_eq!(voltage_with_source_in(&mut port, &mut sess), None);
        assert!(!log.get().iter().any(|c| c.starts_with("0142")));
        // ECU that lists 0x42 (synthetic mask): PID 42 used and labelled as such.
        let (mut port, _log) = ReplayPort::empty()
            .with_reply("0100", &["7E806410000000001"])
            .with_reply("0120", &["7E806412000000001"])
            .with_reply("0140", &["7E806414040000000"])
            .with_reply("01421", &["7E8044142365C"])
            .boxed();
        let mut sess = ElmSession::for_protocol("can");
        let (v, src) = voltage_with_source_in(&mut port, &mut sess).unwrap();
        assert_eq!(src, VoltageSource::Pid42);
        assert!((v - 13.916).abs() < 0.001);
    }

    fn patrol_port_no_atrv() -> (Box<dyn SerialPort + Send>, SentLog) {
        ReplayPort::from_fixture(FIXTURE, "## Section A1").boxed()
    }

    #[test]
    fn replay_mode09_calid_and_blank_vin() {
        let (mut port, _log) = patrol_port();
        let mut sess = ElmSession::for_protocol("can");
        assert_eq!(mode09_in(&mut port, &mut sess, 0x04).unwrap(), "0VS43B");
        assert!(mode09_in(&mut port, &mut sess, 0x02).is_err(), "all-zero VIN is not a VIN");
        assert_eq!(mode09_in(&mut port, &mut sess, 0x0A), Err(ElmError::NoData));
    }

    #[test]
    fn replay_vpw_over_elm_section() {
        let (mut port, log) = ReplayPort::from_fixture(FIXTURE, "## Section C").boxed();
        let mut sess = ElmSession::for_protocol("auto");
        let s = read_support_in(&mut port, &mut sess).unwrap();
        // 0100 -> BFBFF990: includes 01,03,04,05,06,07,09,0C ...
        for p in [0x01u8, 0x03, 0x04, 0x05, 0x06, 0x07, 0x09, 0x0C, 0x0D] {
            assert!(s.contains(&p), "{p:02X}");
        }
        assert!(!sess.can_frames, "J1850 replies are not CAN");
        assert_eq!(mode01_in(&mut port, &mut sess, 0x05).map(|d| decode_ect(&d)), Ok(Some(83.0)));
        assert!(!log.get().iter().any(|c| c == "01051"), "no count suffix on J1850");
    }
}
