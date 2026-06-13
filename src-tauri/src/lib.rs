use pid_decode::*;
use security::{SecurityLevel, SecurityState, unlock_level1, unlock_level2};

use sha2::{Sha256, Digest};
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

#[derive(Serialize, Clone)]
struct EcuProperties {
    os_id:    String,
    vin:      String,
    hardware: String,
    ecu_type: String,
    protocol: String,
    status:   String,
}

#[derive(Serialize, Clone)]
struct PcmBackupResult {
    file_name:  String,
    size_bytes: u32,
    sha256:     String,
    message:    String,
}

#[derive(Serialize, Clone)]
struct BinValidationResult {
    detected_os_id: String,
    checksum_ok:    bool,
    /// Machine-readable gate the frontend keys on. `compatibility` is the
    /// human-readable label; `compatible` is the authoritative boolean.
    compatible:     bool,
    compatibility:  String,
    message:        String,
}

#[derive(Serialize, Clone)]
struct BinCompareResult {
    /// Authoritative boolean gate; `compatibility` is the display label.
    compatible:    bool,
    compatibility: String,
    diff_regions:  u32,
    summary:       String,
}

#[derive(Serialize, Clone)]
struct WriteResult {
    success: bool,
    message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// J1850 VPW frame helpers
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

#[allow(dead_code)]
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
// Serial I/O
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
fn list_supported_ecus() -> Result<Vec<String>, String> {
    Ok(ecu_database::list_supported_ecu_families())
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

#[tauri::command]
fn read_dtcs_cmd(state: tauri::State<AppState>) -> Result<DtcReadResult, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect first".to_string())?;
    read_dtcs(port)
}

#[tauri::command]
fn clear_dtcs_cmd(state: tauri::State<AppState>) -> Result<DtcClearResult, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect first".to_string())?;
    let prior_count = read_dtcs(port).map(|r| r.total).unwrap_or(0);
    clear_dtcs(port, prior_count)
}

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
                "Flash write requires Level 2 security. Call security_unlock_l2 first.".to_string()
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
    poll01!(0x09, ltft_b1_pct,   decode_ltft_b1);
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
// Stage 5: helpers
// ─────────────────────────────────────────────────────────────────────────────

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

fn detect_os_id_from_bytes(data: &[u8]) -> String {
    let cal_a_offset: usize = if data.len() >= 0x28000 { 0x20000 } else { 0 };
    let id_off = cal_a_offset + 0x7FFC;
    if id_off + 4 > data.len() { return "unknown".to_string(); }
    let raw = &data[id_off..id_off + 4];
    if raw.iter().all(|&b| b.is_ascii_digit()) {
        return String::from_utf8_lossy(raw).to_string();
    }
    let num = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
    if num != 0 && num != 0xFFFF_FFFF {
        return format!("{:08X}", num);
    }
    "unknown".to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — Stage 5
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn read_properties(state: tauri::State<AppState>) -> Result<EcuProperties, String> {
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect to the ECU first.".to_string())?;

    let os_id = match request_pid22(port, 0x0100) {
        Ok(r) if r.len() >= 4 => format!("{}", u32::from_be_bytes([r[0], r[1], r[2], r[3]])),
        Ok(r) => format!("short response ({} bytes)", r.len()),
        Err(e) => format!("read error: {}", e),
    };
    let vin = match request_pid22(port, 0x0090) {
        Ok(r) if r.len() >= 17 => String::from_utf8_lossy(&r[..17]).trim().to_string(),
        Ok(r) => format!("short response ({} bytes)", r.len()),
        Err(e) => format!("read error: {}", e),
    };
    let hardware = match request_pid22(port, 0x0050) {
        Ok(r) if r.len() >= 4 => format!("{}", u32::from_be_bytes([r[0], r[1], r[2], r[3]])),
        Ok(r) => format!("short response ({} bytes)", r.len()),
        Err(e) => format!("read error: {}", e),
    };

    Ok(EcuProperties {
        os_id, vin, hardware,
        ecu_type: "P01 / 0411".to_string(),
        protocol: "GM J1850 VPW @ 10.4 kbps".to_string(),
        status:   "Identified".to_string(),
    });
}

#[tauri::command]
fn read_entire_pcm(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<PcmBackupResult, String> {
    use tauri::Emitter;
    use tauri::Manager;
    use std::io::Write as _;

    const FULL_SIZE: u32 = 0x0008_0000;

    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect to the ECU first.".to_string())?;

    let result = flash_read(port, 0x0000_0000, FULL_SIZE, |p: FlashProgress| {
        let _ = app.emit("flash-progress", &p);
    }).map_err(|e| format!("PCM read failed: {}", e))?;

    let hash = sha256_hex(&result.data);
    let ts = chrono::Local::now().format!("%Y%m%d_%H%M%S");
    let file_name = format!("pcm_backup_{}.bin", ts);

    let save_path = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {}", e))
        .map(|p| p.join(&file_name));

    let saved_name = match save_path {
        Ok(path) => {
            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
            match std::fs::File::create(&path) {
                Ok(mut f) => { f.write_all(&result.data).map_err(|e| format!("Write failed: {}", e))?; path.to_string_lossy().to_string() }
                Err(e) => format!("save failed ({}): {}", file_name, e),
            }
        }
        Err(e) => format!("path error: {}", e),
    };

    Ok(PcmBackupResult {
        file_name:  saved_name,
        size_bytes: result.length,
        sha256:     hash,
        message:    format!("Full PCM backup complete. {} bytes read, CRC-32: 0x{:08X}.", result.length, result.crc32),
    })
}

#[tauri::command]
fn validate_bin(file_bytes: Vec<u8>) -> Result<BinValidationResult, String> {
    let size = file_bytes.len();
    let (compatible, compat, checksum_ok, cs_msg) = match size {
        131072 => match validate_checksums(&file_bytes) {
            Ok(report) => {
                let msg = if report.all_valid {
                    format!("All {} checksum regions valid.", report.regions.len())
                } else {
                    format!("{} region(s) invalid.", report.failed_count)
                };
                (true, "Compatible — 128 KiB calibration image", report.all_valid, msg)
            }
            Err(e) => (false, "Incompatible", false, format!("Checksum error: {}", e)),
        },
        524288 => {
            let cal_slice = &file_bytes[0x20000..0x20000 + CAL_IMAGE_SIZE];
            match validate_checksums(cal_slice) {
                Ok(report) => {
                    let msg = if report.all_valid {
                        "Cal region checksums valid (512 KiB full image).".to_string()
                    } else {
                        format!("{} cal region(s) invalid in full image.", report.failed_count)
                    };
                    (true, "Compatible — 512 KiB full PCM image", report.all_valid, msg)
                }
                Err(e) => (false, "Incompatible", false, format!("Checksum error: {}", e)),
            }
        }
        _ => (
            false,
            "Incompatible — unexpected file size",
            false,
            format!("Expected 131072 or 524288 bytes, got {}.", size),
        ),
    };
    let detected_os_id = detect_os_id_from_bytes(&file_bytes);
    let crc = flash_crc32(&file_bytes);
    Ok(BinValidationResult {
        detected_os_id,
        checksum_ok,
        compatible,
        compatibility: compat.to_string(),
        message: format!("{} CRC-32: 0x{:08X}.", cs_msg, crc),
    });
}

#[tauri::command]
fn compare_bin_to_ecu(
    file_bytes: Vec<u8>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<BinCompareResult, String> {
    use tauri::Emitter;
    if file_bytes.len() != CAL_IMAGE_SIZE {
        return Err(format!("Expected {} bytes (128 KiB), got {}.", CAL_IMAGE_SIZE, file_bytes.len()));
    }
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect to the ECU first.".to_string())?;
    let ecu_cal = read_calibration(port, |p: FlashProgress| { let _ = app.emit("flash-progress", &p); })
        .map_err(|e| format!("ECU cal read failed: {}", e))?;
    if ecu_cal.data.len() != CAL_IMAGE_SIZE {
        return Err(format!("ECU returned {} bytes, expected {}. ", ecu_cal.data.len(), CAL_IMAGE_SIZE));
    }
    const BLOCK: usize = 256;
    let total_blocks = CAL_IMAGE_SIZE / BLOCK;
    let mut diff_blocks: u32 = 0;
    for i in 0..total_blocks {
        let start = i * BLOCK;
        let end   = start + BLOCK;
        if file_bytes[start..end] != ecu_cal.data[start..end] { diff_blocks += 1; }
    }
    let pct = (diff_blocks as f32 / total_blocks as f32) * 100.0;
    let (compatible, compat) = if diff_blocks == 0 {
        (true, "Identical")
    } else if pct < 25.0 {
        (true, "Compatible — minor calibration differences")
    } else {
        (false, "Different — significant divergence from ECU")
    };
    Ok(BinCompareResult {
        compatible,
        compatibility: compat.to_string(),
        diff_regions:  diff_blocks,
        summary: format!(
            "{} of {} blocks differ ({:.1}% mismatch). File CRC-32: 0x{:08X}, ECU CRC-32: 0x{:08X}.",
            diff_blocks, total_blocks, pct, flash_crc32(&file_bytes), ecu_cal.crc32
        ),
    });
}

/// write_calibration — calibration-only write path (128 KiB, Cal A+B).
/// Same security gate and checksum correction as write_os_calibration.
/// Called by the frontend "Calibration Only" write mode button.
#[tauri::command]
fn write_calibration_cmd(
    file_bytes: Vec<u8>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<WriteResult, String> {
    use tauri::Emitter;
    {
        let sec = state.security.lock().map_err(|_| "Lock failed".to_string())?;
        if sec.locked || sec.level != Some(SecurityLevel::Level2) {
            return Err("Calibration write requires Level 2 security. Call security_unlock_l2 first.".to_string());
        }
    }
    if file_bytes.len() != CAL_IMAGE_SIZE {
        return Err(format!("Expected 128 KiB calibration image ({} bytes), got {}.", CAL_IMAGE_SIZE, file_bytes.len()));
    }
    let corrected = correct_and_validate_checksums(&file_bytes)
        .map_err(|e| format!("Checksum correction failed: {}", e))?;
    if !corrected.report.all_valid {
        return Err(format!("Checksum correction produced {} invalid region(s). Aborting.", corrected.report.failed_count));
    }
    let fixed = corrected.report.fixed_count;
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect to the ECU first.".to_string())?;
    let write_result = write_calibration(port, &corrected.data, |p: FlashProgress| {
        let _ = app.emit("flash-progress", &p);
    }).map_err(|e| format!("Flash write failed: {}", e))?;
    Ok(WriteResult {
        success: true,
        message: format!(
            "Calibration written successfully. {} bytes, {} blocks. {} region(s) checksum-corrected. CRC-32: 0x{:08X}.",
            write_result.bytes_written, write_result.blocks_written, fixed, write_result.crc32_written,
        ),
    });
}

#[tauri::command]
fn write_os_calibration(
    file_bytes: Vec<u8>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<WriteResult, String> {
    use tauri::Emitter;
    {
        let sec = state.security.lock().map_err(|_| "Lock failed".to_string())?;
        if sec.locked || sec.level != Some(SecurityLevel::Level2) {
            return Err("Calibration write requires Level 2 security. Call security_unlock_l2 first.".to_string());
        }
    }
    if file_bytes.len() == 524288 {
        return Err(
            "Full 512 KiB OS+Cal images are not supported by write_os_calibration. \
             Use flash_write_region for OS-level access.".to_string()
        );
    }
    if file_bytes.len() != CAL_IMAGE_SIZE {
        return Err(format!("Expected 128 KiB calibration image ({} bytes), got {}.", CAL_IMAGE_SIZE, file_bytes.len()));
    }
    let corrected = correct_and_validate_checksums(&file_bytes)
        .map_err(|e| format!("Checksum correction failed: {}", e))?;
    if !corrected.report.all_valid {
        return Err(format!("Checksum correction produced {} invalid region(s). Aborting flash write.", corrected.report.failed_count));
    }
    let fixed = corrected.report.fixed_count;
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect to the ECU first.".to_string())?;
    let write_result = write_calibration(port, &corrected.data, |p: FlashProgress| {
        let _ = app.emit("flash-progress", &p);
    }).map_err(|e| format!("Flash write failed: {}", e))?;
    Ok(WriteResult {
        success: true,
        message: format!(
            "Calibration written successfully. {} bytes in {} blocks. {} checksum region(s) corrected pre-write. CRC-32: 0x{:08X}.",
            write_result.bytes_written, write_result.blocks_written, fixed, write_result.crc32_written,
        ),
    });
}

#[tauri::command]
fn verify_after_write(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<WriteResult, String> {
    use tauri::Emitter;
    let mut port_guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = port_guard.as_mut().ok_or("No connection — connect to the ECU first.".to_string())?;
    let readback = read_calibration(port, |p: FlashProgress| { let _ = app.emit("flash-progress", &p); })
        .map_err(|e| format!("Readback failed: {}", e))?;
    let hash = sha256_hex(&readback.data);
    let report = validate_checksums(&readback.data)
        .map_err(|e| format!("Checksum validation failed: {}", e))?;
    if report.all_valid {
        Ok(WriteResult {
            success: true,
            message: format!(
                "Verification passed. All {} checksum regions valid. {} bytes read back. SHA-256: {}.",
                report.regions.len(), readback.length, hash,
            ),
        });
    } else {
        Ok(WriteResult {
            success: false,
            message: format!(
                "Verification FAILED. {} of {} checksum regions invalid. SHA-256: {}.",
                report.failed_count, report.regions.len(), hash,
            ),
        });
    }
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
            list_serial_ports,
            list_supported_ecus,
            connect_ecu,
            disconnect_ecu,
            connection_status,
            security_unlock_l1,
            security_unlock_l2,
            security_status,
            read_dtcs_cmd,
            clear_dtcs_cmd,
            read_freeze_frame_cmd,
            validate_cal_checksum,
            correct_cal_checksum,
            flash_read_region,
            flash_read_cal,
            flash_write_region,
            flash_write_cal,
            write_ecu_frame,
            read_ecu_frame,
            read_ecu_data,
            read_properties,
            read_entire_pcm,
            validate_bin,
            compare_bin_to_ecu,
            write_calibration_cmd,   // calibration-only write (frontend: write_calibration)
            write_os_calibration,
            verify_after_write,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri runtime error");
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — command contracts for the offline (no-hardware) code paths
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── VPW frame builders & checksum ──────────────────────────────────────

    #[test]
    fn pid_request_has_valid_checksum() {
        let frame = build_pid_request(0x0C);
        assert_eq!(&frame[..5], &[0x68, 0x6A, 0xF1, 0x01, 0x0C]);
        assert!(validate_checksum(&frame));
    }

    #[test]
    fn mode22_request_has_valid_checksum() {
        let frame = build_mode22_request(0x1141);
        assert_eq!(&frame[..6], &[0x68, 0x6A, 0xF1, 0x22, 0x11, 0x41]);
        assert!(validate_checksum(&frame));
    }

    #[test]
    fn checksum_rejects_corrupt_frame() {
        let mut frame = build_pid_request(0x05);
        *frame.last_mut().unwrap() ^= 0xFF;
        assert!(!validate_checksum(&frame));
    }

    // ── OS-ID detection ────────────────────────────────────────────────────

    #[test]
    fn os_id_unknown_for_blank_image() {
        let img = vec![0u8; 131072];
        assert_eq!(detect_os_id_from_bytes(&img), "unknown");
    }

    #[test]
    fn os_id_decodes_ascii_digits() {
        let mut img = vec![0u8; 131072];
        img[0x7FFC..0x8000].copy_from_slice(b"1234");
        assert_eq!(detect_os_id_from_bytes(&img), "1234");
    }

    #[test]
    fn os_id_handles_full_image_offset() {
        // 512 KiB image: cal A starts at 0x20000, id at +0x7FFC.
        let mut img = vec![0u8; 524288];
        img[0x20000 + 0x7FFC..0x20000 + 0x8000].copy_from_slice(b"9876");
        assert_eq!(detect_os_id_from_bytes(&img), "9876");
    }

    // ── validate_bin contract: size gating + `compatible` boolean ───────────

    #[test]
    fn validate_bin_rejects_bad_size() {
        let r = validate_bin(vec![0u8; 1000]).unwrap();
        assert!(!r.compatible);
        assert!(!r.checksum_ok);
        assert!(r.compatibility.contains("unexpected file size"));
    }

    #[test]
    fn validate_bin_accepts_128k_size() {
        // A blank 128 KiB image is size-compatible even if checksums fail.
        let r = validate_bin(vec![0u8; 131072]).unwrap();
        assert!(r.compatible, "128 KiB image must be size-compatible");
        assert!(r.compatibility.starts_with("Compatible"));
    }

    #[test]
    fn validate_bin_accepts_512k_size() {
        let r = validate_bin(vec![0u8; 524288]).unwrap();
        assert!(r.compatible, "512 KiB full image must be size-compatible");
        assert!(r.compatibility.contains("512 KiB"));
    }

    #[test]
    fn validate_bin_surfaces_detected_os_id() {
        let mut img = vec![0u8; 131072];
        img[0x7FFC..0x8000].copy_from_slice(b"5566");
        let r = validate_bin(img).unwrap();
        assert_eq!(r.detected_os_id, "5566");
        assert!(r.compatible);
    }
}