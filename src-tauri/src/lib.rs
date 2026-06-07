//! TuneItVerse — LS1 P01 PCM backend (Tauri)
//!
//! Stage 2:  Full VPW PID decode via pid_decode.rs
//! Stage 2b: GM P01 seed-key security unlock via security.rs
//! Stage 3:  Mode 23/34/36/37 flash read/write via flash.rs
//! Stage 3b: P01 calibration checksum correction via checksum.rs
//! Stage 4:  DTC read / clear / freeze frame via dtc.rs
//! Stage 5:  ECU identification, full PCM backup, BIN validation via stubs below
//! Protocol: J1850 VPW via USB-serial bridge @ 115200 baud

pub mod checksum;
pub mod dtc;
pub mod flash;
pub mod pid_decode;
pub mod security;
pub mod vpw;

use checksum::{
    ChecksumReport, CorrectedCal,
    validate_checksums, correct_and_validate_checksums,
    CAL_IMAGE_SIZE,
};
use dtc::{
    DtcReadResult, DtcClearResult, FreezeFrameResult,
    read_dtcs, clear_dtcs, read_freeze_frame,
};
use flash::{
    FlashProgress, FlashReadResult, FlashWriteResult,
    flash_read, flash_write, read_calibration, write_calibration,
    guard_write_range, CAL_A_START, CAL_REGION_SIZE,
};
use pid_decode::*;
use security::{SecurityLevel, SecurityState, unlock_level1, unlock_level2};

use serde::Serialize;
use serialport::{available_ports, SerialPort};
use std::{
    io::{Read, Write},
    sync::Mutex,
    time::Duration,
};

// ─────────────────────────────────────────────────────────────────────────────
// App state
// ─────────────────────────────────────────────────────────────────────────────

struct AppState {
    port:     Mutex<Option<Box<dyn SerialPort>>>,
    security: Mutex<SecurityState>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct SerialPortInfo {
    port_name: String,
    port_type: String,
}

#[derive(Serialize, Clone)]
struct RawFrame {
    raw:        Vec<u8>,
    hex:        String,
    bytes_read: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Stage 5 wire types — identification, backup, BIN validation
// ─────────────────────────────────────────────────────────────────────────────

/// Returned by `read_properties` — ECU identity fields read via Mode 22 DIDs.
#[derive(Serialize, Clone)]
struct EcuProperties {
    os_id:    String,
    vin:      String,
    hardware: String,
    ecu_type: String,
    protocol: String,
    status:   String,
}

/// Returned by `read_entire_pcm` — full 512 KiB flash backup result.
#[derive(Serialize, Clone)]
struct PcmBackupResult {
    file_name:  String,
    size_bytes: u32,
    sha256:     String,
    message:    String,
}

/// Returned by `validate_bin` — pre-flash preflight checks.
#[derive(Serialize, Clone)]
struct BinValidationResult {
    detected_os_id: String,
    checksum_ok:    bool,
    compatibility:  String,
    message:        String,
}

/// Returned by `compare_bin_to_ecu` — diff summary.
#[derive(Serialize, Clone)]
struct BinCompareResult {
    compatibility: String,
    diff_regions:  u32,
    summary:       String,
}

/// Returned by `write_os_calibration` and `verify_after_write`.
#[derive(Serialize, Clone)]
struct WriteResult {
    success: bool,
    message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// J1850 VPW frame helpers  (pub(crate) so security.rs + flash.rs + dtc.rs)
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn build_pid_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x01, pid];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

pub(crate) fn build_mode22_request(pid: u16) -> Vec<u8> {
    let mut frame = vec![
        0x68u8, 0x6A, 0xF1, 0x22,
        (pid >> 8) as u8,
        (pid & 0xFF) as u8,
    ];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

fn build_multi_pid_request(pids: &[u8]) -> Vec<u8> {
    assert!(!pids.is_empty() && pids.len() <= 6);
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x01];
    frame.extend_from_slice(pids);
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

pub(crate) fn validate_checksum(frame: &[u8]) -> bool {
    if frame.len() < 2 { return false; }
    frame[..frame.len()-1].iter().fold(0u8, |a, &b| a.wrapping_add(b))
        == frame[frame.len()-1]
}

fn parse_pid_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 { return None; }
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_checksum(frame) {
        return Some(frame[5..frame.len()-1].to_vec());
    }
    None
}

fn parse_mode22_response(frame: &[u8], expected_pid: u16) -> Option<Vec<u8>> {
    if frame.len() < 7 { return None; }
    let pid_hi = (expected_pid >> 8) as u8;
    let pid_lo = (expected_pid & 0xFF) as u8;
    if frame[3] == 0x62 && frame[4] == pid_hi && frame[5] == pid_lo
        && validate_checksum(frame)
    {
        return Some(frame[6..frame.len()-1].to_vec());
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Serial I/O  (pub(crate))
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn write_frame(port: &mut Box<dyn SerialPort>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame).map_err(|e| format!("Write error: {}", e))
}

pub(crate) fn read_response(port: &mut Box<dyn SerialPort>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 256];
    let n = port.read(&mut buf).map_err(|e| format!("Read error: {}", e))?;
    Ok(buf[..n].to_vec())
}

fn request_pid01(port: &mut Box<dyn SerialPort>, pid: u8) -> Result<Vec<u8>, String> {
    write_frame(port, &build_pid_request(pid))?;
    let resp = read_response(port)?;
    parse_pid_response(&resp, pid)
        .ok_or_else(|| format!("No valid Mode01 response for PID 0x{:02X}", pid))
}

fn request_pid22(port: &mut Box<dyn SerialPort>, pid: u16) -> Result<Vec<u8>, String> {
    write_frame(port, &build_mode22_request(pid))?;
    let resp = read_response(port)?;
    parse_mode22_response(&resp, pid)
        .ok_or_else(|| format!("No valid Mode22 response for PID 0x{:04X}", pid))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — connection management
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn list_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
    let ports = available_ports().map_err(|e| e.to_string())?;
    Ok(ports.into_iter().map(|p| SerialPortInfo {
        port_name: p.port_name,
        port_type: format!("{:?}", p.port_type),
    }).collect())
}

#[tauri::command]
fn connect_ecu(port: String, baud: u32, state: tauri::State<AppState>) -> Result<String, String> {
    let serial = serialport::new(&port, baud)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| format!("Failed to open {}: {}", port, e))?;
    *state.port.lock().map_err(|_| "Lock failed".to_string())? = Some(serial);
    *state.security.lock().map_err(|_| "Lock failed".to_string())? = SecurityState::default();
    Ok(format!("Connected to {} @ {} baud", port, baud))
}

#[tauri::command]
fn disconnect_ecu(state: tauri::State<AppState>) -> Result<String, String> {
    *state.port.lock().map_err(|_| "Lock failed".to_string())? = None;
    *state.security.lock().map_err(|_| "Lock failed".to_string())? = SecurityState::default();
    Ok("Disconnected".to_string())
}

#[tauri::command]
fn connection_status(state: tauri::State<AppState>) -> Result<bool, String> {
    Ok(state.port.lock().map_err(|_| "Lock failed".to_string())?.is_some())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — security access
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn security_unlock_l1(state: tauri::State<AppState>) -> Result<SecurityState, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect first".to_string())?;
    let result = unlock_level1(port)?;
    *state.security.lock().map_err(|_| "Lock failed".to_string())? = result.clone();
    Ok(result)
}

#[tauri::command]
fn security_unlock_l2(state: tauri::State<AppState>) -> Result<SecurityState, String> {
    {
        let sec = state.security.lock().map_err(|_| "Lock failed".to_string())?;
        if sec.locked || sec.level != Some(SecurityLevel::Level1) {
            return Err(
                "Level 1 must be unlocked before requesting Level 2. \
                 Call security_unlock_l1 first.".to_string()
            );
        }
    }
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection".to_string())?;
    let result = unlock_level2(port)?;
    *state.security.lock().map_err(|_| "Lock failed".to_string())? = result.clone();
    Ok(result)
}

#[tauri::command]
fn security_status(state: tauri::State<AppState>) -> Result<SecurityState, String> {
    Ok(state.security.lock().map_err(|_| "Lock failed".to_string())?.clone())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — DTC read / clear / freeze frame
// ─────────────────────────────────────────────────────────────────────────────

/// Read all DTCs from the ECM (stored, pending, permanent).
/// Returns a DtcReadResult with all three groups and descriptions.
/// Requires: ECU connected. Level 1 security recommended.
#[tauri::command]
fn read_dtcs_cmd(state: tauri::State<AppState>) -> Result<DtcReadResult, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect first".to_string())?;
    read_dtcs(port)
}

/// Clear all DTCs and reset readiness monitors (Mode 04).
///
/// ⚠️  This is destructive — all stored/pending DTCs and freeze frame
///    data will be erased. Readiness monitors will reset.
/// ⚠️  Does NOT require Level 2 — Mode 04 is a standard OBD-II service.
#[tauri::command]
fn clear_dtcs_cmd(state: tauri::State<AppState>) -> Result<DtcClearResult, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect first".to_string())?;

    // Read current count first so the result message is informative
    let prior_count = read_dtcs(port)
        .map(|r| r.total)
        .unwrap_or(0);

    clear_dtcs(port, prior_count)
}

/// Read freeze frame data (Mode 02) — snapshot captured when the first DTC was set.
/// Returns sensor values at the moment of fault.
#[tauri::command]
fn read_freeze_frame_cmd(state: tauri::State<AppState>) -> Result<FreezeFrameResult, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect first".to_string())?;
    read_freeze_frame(port)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — checksum validation / correction (offline)
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn validate_cal_checksum(data: Vec<u8>) -> Result<ChecksumReport, String> {
    validate_checksums(&data)
}

#[tauri::command]
fn correct_cal_checksum(data: Vec<u8>) -> Result<CorrectedCal, String> {
    correct_and_validate_checksums(&data)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — flash read (Mode 23)
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn flash_read_region(
    start_addr: u32,
    length: u32,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<FlashReadResult, String> {
    use tauri::Emitter;
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection".to_string())?;
    flash_read(port, start_addr, length, |p| {
        let _ = app.emit("flash-progress", &p);
    })
}

#[tauri::command]
fn flash_read_cal(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<FlashReadResult, String> {
    use tauri::Emitter;
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection".to_string())?;
    read_calibration(port, |p| {
        let _ = app.emit("flash-progress", &p);
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — flash write (Mode 34/36/37)
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn flash_write_region(
    start_addr: u32,
    data: Vec<u8>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<FlashWriteResult, String> {
    use tauri::Emitter;
    {
        let sec = state.security.lock().map_err(|_| "Lock failed".to_string())?;
        if sec.locked || sec.level != Some(SecurityLevel::Level2) {
            return Err(
                "Flash write requires Level 2 security. \
                 Call security_unlock_l2 first.".to_string()
            );
        }
    }
    guard_write_range(start_addr, data.len() as u32)?;
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection".to_string())?;
    flash_write(port, start_addr, &data, |p| {
        let _ = app.emit("flash-progress", &p);
    })
}

#[tauri::command]
fn flash_write_cal(
    data: Vec<u8>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<FlashWriteResult, String> {
    use tauri::Emitter;
    {
        let sec = state.security.lock().map_err(|_| "Lock failed".to_string())?;
        if sec.locked || sec.level != Some(SecurityLevel::Level2) {
            return Err(
                "Flash write requires Level 2 security. \
                 Call security_unlock_l2 first.".to_string()
            );
        }
    }
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection".to_string())?;
    write_calibration(port, &data, |p| {
        let _ = app.emit("flash-progress", &p);
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — raw frame I/O
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn write_ecu_frame(data: Vec<u8>, state: tauri::State<AppState>) -> Result<String, String> {
    let mut guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No connection".to_string())?;
    write_frame(port, &data)?;
    Ok(format!("Wrote {} bytes", data.len()))
}

#[tauri::command]
fn read_ecu_frame(state: tauri::State<AppState>) -> Result<RawFrame, String> {
    let mut guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No connection".to_string())?;
    let raw = read_response(port)?;
    let hex = raw.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
    Ok(RawFrame { bytes_read: raw.len(), raw, hex })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri command — full ECU telemetry poll
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn read_ecu_data(state: tauri::State<AppState>) -> Result<EcuTelemetry, String> {
    let mut guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No connection".to_string())?;

    let mut d = EcuTelemetry::default();
    d.batt_volt = 12.0;
    d.wb_afr    = 14.7;

    macro_rules! poll01 {
        ($pid:expr, $field:ident, $decoder:expr) => {
            if let Ok(r) = request_pid01(port, $pid) {
                if let Some(v) = $decoder(&r) { d.$field = v; }
            }
        };
    }
    macro_rules! poll22 {
        ($pid:expr, $field:ident, $decoder:expr) => {
            if let Ok(r) = request_pid22(port, $pid) {
                if let Some(v) = $decoder(&r) { d.$field = v; }
            }
        };
    }

    if let Ok(r) = request_pid01(port, 0x01) {
        if let Some((mil, cnt)) = decode_monitor_status(&r) {
            d.mil_on    = mil;
            d.dtc_count = cnt;
        }
    }
    poll01!(0x04, engine_load,   decode_engine_load);
    poll01!(0x05, ect_c,         decode_ect);
    poll01!(0x06, stft_b1_pct,   decode_stft_b1);
    poll01!(0x07, ltft_b1_pct,   decode_ltft_b1);
    poll01!(0x08, stft_b2_pct,   decode_stft_b2);
    poll01!(0x09, ltft_b2_pct,   decode_ltft_b2);
    poll01!(0x0B, map_kpa,       decode_map);
    poll01!(0x0C, rpm,           decode_rpm);
    poll01!(0x0D, vss_kph,       decode_vss);
    poll01!(0x0E, spark_adv_deg, decode_spark_adv);
    poll01!(0x0F, iat_c,         decode_iat);
    poll01!(0x10, maf_gs,        decode_maf);
    poll01!(0x11, tps_pct,       decode_tps);

    if let Ok(r) = request_pid01(port, 0x14) { if let Some(v) = decode_o2_left_up(&r)  { d.o2_left_up_v  = v; } }
    if let Ok(r) = request_pid01(port, 0x15) { if let Some(v) = decode_o2_left_dn(&r)  { d.o2_left_dn_v  = v; } }
    if let Ok(r) = request_pid01(port, 0x18) { if let Some(v) = decode_o2_right_up(&r) { d.o2_right_up_v = v; } }
    if let Ok(r) = request_pid01(port, 0x19) { if let Some(v) = decode_o2_right_dn(&r) { d.o2_right_dn_v = v; } }

    poll22!(0x1140, maf_hi_gs,      decode_maf_hi);
    poll22!(0x1141, batt_volt,      decode_batt_volt);
    poll22!(0x1142, map_volts,      decode_map_volts);
    poll22!(0x1145, o2_lf_mv,       decode_o2_lf_mv);
    poll22!(0x1146, o2_lr_mv,       decode_o2_lr_mv);
    poll22!(0x1148, o2_rf_mv,       decode_o2_rf_mv);
    poll22!(0x1149, o2_rr_mv,       decode_o2_rr_mv);
    poll22!(0x1170, evap_purge_pct, decode_evap_purge);
    poll22!(0x1176, iac_learned,    decode_iac_learned);
    poll22!(0x1179, iac_current,    decode_iac_current);
    poll22!(0x1190, fuel_trim_cell, decode_fuel_trim_cell);
    poll22!(0x1192, idle_desired,   decode_desired_idle);
    poll22!(0x119D, baro_kpa,       decode_baro);
    poll22!(0x119E, target_afr,     decode_target_afr);
    poll22!(0x119F, oil_life_pct,   decode_oil_life);
    poll22!(0x11A1, engine_run_min, decode_run_time_min);
    poll22!(0x11A3, cat_temp_c,     decode_cat_temp);
    poll22!(0x11A6, knock_retard,   decode_knock_retard);
    poll22!(0x116F, startup_ect_c,  decode_startup_ect);
    poll22!(0x115C, oil_press_kpa,  decode_oil_press_kpa);
    poll22!(0x125A, inj_pw_b1_ms,   decode_inj_pw);
    poll22!(0x125B, inj_pw_b2_ms,   decode_inj_pw);
    poll22!(0x11EA, misfire_c5,     decode_misfire);
    poll22!(0x11EB, misfire_c6,     decode_misfire);
    poll22!(0x11EC, misfire_c7,     decode_misfire);
    poll22!(0x11ED, misfire_c8,     decode_misfire);
    poll22!(0x19F3, trans_oil_temp, decode_trans_oil_temp);
    poll22!(0x19F4, trans_ratio,    decode_trans_ratio);
    poll22!(0x19F5, trans_gear,     decode_trans_gear);
    poll22!(0x199A, current_gear,   decode_current_gear);

    if let Ok(r) = request_pid22(port, 0x1100) {
        if let Some(&b) = r.first() {
            d.ac_relay   = decode_ac_relay(b);
            d.ac_request = decode_ac_request(b);
            d.ac_clutch  = decode_ac_clutch(b);
            d.pcm_reset  = decode_pcm_reset(b);
        }
    }
    if let Ok(r) = request_pid22(port, 0x1101) {
        if let Some(&b) = r.first() { d.evap_vent = decode_evap_vent(b); }
    }
    if let Ok(r) = request_pid22(port, 0x1102) {
        if let Some(&b) = r.first() {
            d.vtd_fuel_dis  = decode_vtd_fuel_disable(b);
            d.tcc_solenoid  = decode_tcc_solenoid(b);
            d.traction_ctrl = decode_traction_ctrl(b);
        }
    }
    if let Ok(r) = request_pid22(port, 0x1103) {
        if let Some(&b) = r.first() {
            d.reduced_power = decode_reduced_power(b);
            d.oil_level_low = decode_oil_level_low(b);
            d.mil_cmd       = decode_mil_cmd(b);
        }
    }
    if let Ok(r) = request_pid22(port, 0x1104) {
        if let Some(&b) = r.first() {
            d.cruise_active    = decode_cruise_active(b);
            d.dtc_set_this_ign = decode_dtc_set_this_ign(b);
        }
    }
    if let Ok(r) = request_pid22(port, 0x1105) {
        if let Some(&b) = r.first() {
            d.loop_closed     = decode_loop_closed(b);
            d.fuel_trim_learn = decode_fuel_trim_learn(b);
            d.cold_startup    = decode_cold_startup(b);
            d.decel_active    = decode_decel(b);
        }
    }
    if let Ok(r) = request_pid22(port, 0x1115) {
        if let Some(&b) = r.first() { d.tac_comm_good = decode_tac_comm(b); }
    }

    d.idc_b1_pct = calc_idc_b1(d.rpm, d.inj_pw_b1_ms);
    Ok(d)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Stage 5: ECU identification, PCM backup, BIN validation
//
// These are currently stubs that return sensible placeholder data so the
// frontend Read/Write pipeline works end-to-end without panicking.
// TODO: wire each to real VPW Mode 22 reads, flash_read, and checksum logic.
// ─────────────────────────────────────────────────────────────────────────────

/// Read ECU identity properties via Mode 22 DIDs.
///
/// DIDs used on P01/P59:
///   0x0100 — OS ID (calibration part number)
///   0x0090 — VIN (17-byte ASCII)
///   0x0050 — Hardware part number
/// TODO: implement real DID reads via vpw::build_mode22_request.
#[tauri::command]
fn read_properties(state: tauri::State<AppState>) -> Result<EcuProperties, String> {
    let guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    if guard.is_none() {
        return Err("No connection — connect to the ECU first.".to_string());
    }
    // Stub: returns placeholder data. Replace with real Mode 22 DID reads.
    Ok(EcuProperties {
        os_id:    "12225074".to_string(),
        vin:      "1G1YY22G615100001".to_string(),
        hardware: "09354896".to_string(),
        ecu_type: "P01 / 0411".to_string(),
        protocol: "GM J1850 VPW @ 10.4 kbps".to_string(),
        status:   "Identified (stub)".to_string(),
    })
}

/// Read entire PCM flash (512 KiB) via Mode 23 and save to a timestamped file.
///
/// Full read uses flash::flash_read with address range 0x00000–0x7FFFF.
/// Result is SHA-256 hashed for integrity verification.
/// TODO: implement real flash read and file save with chrono timestamp.
#[tauri::command]
fn read_entire_pcm(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<PcmBackupResult, String> {
    use tauri::Emitter;
    let guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    if guard.is_none() {
        return Err("No connection — connect to the ECU first.".to_string());
    }
    // Emit a placeholder progress event so the UI progress bar activates.
    let _ = app.emit("flash-progress", serde_json::json!({
        "percent": 0,
        "bytes_done": 0,
        "total_bytes": 524288,
        "phase": "Reading full PCM (stub)"
    }));
    // Stub: returns placeholder data. Replace with flash_read(0x00000, 0x80000).
    Ok(PcmBackupResult {
        file_name:  "backup_stub.bin".to_string(),
        size_bytes: 524288,
        sha256:     "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        message:    "Stub backup complete — wire to flash::flash_read for real data.".to_string(),
    })
}

/// Validate a BIN file before writing — check size, OS ID, and P01 checksum.
///
/// Valid P01 cal images are exactly 512 KiB (0x80000 bytes).
/// Checksum is validated via checksum::validate_checksums.
/// TODO: read file bytes from disk (via tauri::api::path) and run real validation.
#[tauri::command]
fn validate_bin(
    file_name: String,
    file_size: u64,
) -> Result<BinValidationResult, String> {
    // Basic size check — P01 calibration region is 128 KiB, full image 512 KiB.
    let compat = if file_size == 524288 || file_size == 131072 {
        "Compatible"
    } else {
        "Incompatible — unexpected file size"
    };
    // Stub: returns placeholder data. Replace with checksum::validate_checksums(bytes).
    Ok(BinValidationResult {
        detected_os_id: "12225074 (stub)".to_string(),
        checksum_ok:    file_size == 524288 || file_size == 131072,
        compatibility:  compat.to_string(),
        message:        format!("Stub validation of {} ({} bytes).", file_name, file_size),
    })
}

/// Compare a BIN file to the currently flashed ECU calibration.
///
/// Reads calibration region from ECU via flash_read_cal, then diffs
/// against supplied file bytes block-by-block.
/// TODO: implement real diff using flash::read_calibration.
#[tauri::command]
fn compare_bin_to_ecu(
    file_name: String,
    file_size: u64,
    state: tauri::State<AppState>,
) -> Result<BinCompareResult, String> {
    let guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    if guard.is_none() {
        return Err("No connection — connect to the ECU first.".to_string());
    }
    // Stub: returns placeholder data.
    Ok(BinCompareResult {
        compatibility: "Compatible".to_string(),
        diff_regions:  0,
        summary:       format!(
            "Stub compare: {} ({} bytes) vs ECU — 0 diff regions found.",
            file_name, file_size
        ),
    })
}

/// Write both OS and calibration regions (full re-flash).
///
/// Requires Level 2 security. Uses Mode 34/36/37 flash write sequence.
/// This is a higher-risk operation than calibration-only write.
/// TODO: wire to flash::flash_write for OS region + flash::write_calibration.
#[tauri::command]
fn write_os_calibration(
    file_name: String,
    state: tauri::State<AppState>,
) -> Result<WriteResult, String> {
    {
        let sec = state.security.lock().map_err(|_| "Lock failed".to_string())?;
        if sec.locked || sec.level != Some(SecurityLevel::Level2) {
            return Err(
                "Full OS+Cal write requires Level 2 security. \
                 Call security_unlock_l2 first.".to_string()
            );
        }
    }
    // Stub: returns placeholder success.
    Ok(WriteResult {
        success: true,
        message: format!("Stub OS+Cal write of {} complete — wire to flash::flash_write.", file_name),
    })
}

/// Verify the written calibration by reading it back and comparing checksums.
///
/// Called automatically after write_os_calibration or write_calibration.
/// TODO: implement real readback + SHA-256 compare.
#[tauri::command]
fn verify_after_write(state: tauri::State<AppState>) -> Result<WriteResult, String> {
    let guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    if guard.is_none() {
        return Err("No connection — connect to the ECU first.".to_string());
    }
    // Stub: returns placeholder success.
    Ok(WriteResult {
        success: true,
        message: "Stub verification passed — wire to flash_read + checksum compare.".to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// App entry point
// ─────────────────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            port:     Mutex::new(None),
            security: Mutex::new(SecurityState::default()),
        })
        .invoke_handler(tauri::generate_handler![
            // Connection
            list_serial_ports,
            connect_ecu,
            disconnect_ecu,
            connection_status,
            // Security
            security_unlock_l1,
            security_unlock_l2,
            security_status,
            // DTC
            read_dtcs_cmd,
            clear_dtcs_cmd,
            read_freeze_frame_cmd,
            // Checksum (offline)
            validate_cal_checksum,
            correct_cal_checksum,
            // Flash read
            flash_read_region,
            flash_read_cal,
            // Flash write
            flash_write_region,
            flash_write_cal,
            // Raw I/O
            write_ecu_frame,
            read_ecu_frame,
            // Telemetry
            read_ecu_data,
            // Stage 5 — identification, backup, BIN validation
            read_properties,
            read_entire_pcm,
            validate_bin,
            compare_bin_to_ecu,
            write_os_calibration,
            verify_after_write,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri runtime error");
}
