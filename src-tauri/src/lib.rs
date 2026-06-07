//! TuneItVerse — LS1 P01 PCM backend (Tauri)
//!
//! Stage 2: Full VPW PID decode via pid_decode.rs
//! Protocol: J1850 VPW via USB-serial bridge @ 115200 baud
//! PID source-of-truth: PidParameters-VPW.XML (GM Gen III V8)

pub mod pid_decode;
use pid_decode::*;

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
    port: Mutex<Option<Box<dyn SerialPort>>>,
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
// J1850 VPW frame helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a Mode 01 (OBD-II) single-PID request.
/// Frame: [ 0x68 0x6A 0xF1 0x01 <pid> <checksum> ]
fn build_pid_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x01, pid];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

/// Build a Mode 22 (GM 2-byte PID) request.
/// Frame: [ 0x68 0x6A 0xF1 0x22 <pid_hi> <pid_lo> <checksum> ]
fn build_mode22_request(pid: u16) -> Vec<u8> {
    let mut frame = vec![
        0x68u8, 0x6A, 0xF1, 0x22,
        (pid >> 8) as u8,
        (pid & 0xFF) as u8,
    ];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

/// Build a multi-PID Mode 01 request (up to 6 PIDs at once).
fn build_multi_pid_request(pids: &[u8]) -> Vec<u8> {
    assert!(!pids.is_empty() && pids.len() <= 6);
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x01];
    frame.extend_from_slice(pids);
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

fn validate_checksum(frame: &[u8]) -> bool {
    if frame.len() < 2 { return false; }
    let expected = frame[..frame.len() - 1]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_add(b));
    expected == frame[frame.len() - 1]
}

/// Parse a Mode 01 (0x41) response and return the data bytes.
fn parse_pid_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 { return None; }
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_checksum(frame) {
        return Some(frame[5..frame.len() - 1].to_vec());
    }
    None
}

/// Parse a Mode 22 (0x62) response and return the data bytes.
fn parse_mode22_response(frame: &[u8], expected_pid: u16) -> Option<Vec<u8>> {
    if frame.len() < 7 { return None; }
    let pid_hi = (expected_pid >> 8) as u8;
    let pid_lo = (expected_pid & 0xFF) as u8;
    if frame[3] == 0x62
        && frame[4] == pid_hi
        && frame[5] == pid_lo
        && validate_checksum(frame)
    {
        return Some(frame[6..frame.len() - 1].to_vec());
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Serial I/O
// ─────────────────────────────────────────────────────────────────────────────

fn write_frame(port: &mut Box<dyn SerialPort>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame).map_err(|e| format!("Write error: {}", e))
}

fn read_response(port: &mut Box<dyn SerialPort>) -> Result<Vec<u8>, String> {
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
    Ok(ports
        .into_iter()
        .map(|p| SerialPortInfo {
            port_name: p.port_name,
            port_type: format!("{:?}", p.port_type),
        })
        .collect())
}

#[tauri::command]
fn connect_ecu(
    port: String,
    baud: u32,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    let serial = serialport::new(&port, baud)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| format!("Failed to open {}: {}", port, e))?;
    *state.port.lock().map_err(|_| "Lock failed".to_string())? = Some(serial);
    Ok(format!("Connected to {} @ {} baud", port, baud))
}

#[tauri::command]
fn disconnect_ecu(state: tauri::State<AppState>) -> Result<String, String> {
    *state.port.lock().map_err(|_| "Lock failed".to_string())? = None;
    Ok("Disconnected".to_string())
}

#[tauri::command]
fn connection_status(state: tauri::State<AppState>) -> Result<bool, String> {
    Ok(state.port.lock().map_err(|_| "Lock failed".to_string())?.is_some())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands — raw frame I/O
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn write_ecu_frame(
    data: Vec<u8>,
    state: tauri::State<AppState>,
) -> Result<String, String> {
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
//
// Polls Mode 01 (standard OBD-II) and Mode 22 (GM-specific) PIDs in sequence.
// Falls back to default values for any PID that doesn't respond.
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn read_ecu_data(state: tauri::State<AppState>) -> Result<EcuTelemetry, String> {
    let mut guard = state.port.lock().map_err(|_| "Lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No connection".to_string())?;

    let mut d = EcuTelemetry::default();
    d.batt_volt = 12.0;
    d.wb_afr    = 14.7;

    // ── Mode 01 standard PIDs ────────────────────────────────────────────
    macro_rules! poll01 {
        ($pid:expr, $field:ident, $decoder:expr) => {
            if let Ok(r) = request_pid01(port, $pid) {
                if let Some(v) = $decoder(&r) { d.$field = v; }
            }
        };
    }
    macro_rules! poll01_bit {
        ($pid:expr, $decoder:expr) => {{
            request_pid01(port, $pid).ok().and_then(|r| r.first().copied())
        }};
    }

    poll01!(0x01, mil_on,      |r: &[u8]| decode_monitor_status(r).map(|(mil,_)| mil as u8 as f32));
    if let Ok(r) = request_pid01(port, 0x01) {
        if let Some((mil, cnt)) = decode_monitor_status(&r) {
            d.mil_on    = mil;
            d.dtc_count = cnt;
        }
    }
    poll01!(0x04, engine_load, decode_engine_load);
    poll01!(0x05, ect_c,       decode_ect);
    poll01!(0x06, stft_b1_pct, decode_stft_b1);
    poll01!(0x07, ltft_b1_pct, decode_ltft_b1);
    poll01!(0x08, stft_b2_pct, decode_stft_b2);
    poll01!(0x09, ltft_b2_pct, decode_ltft_b2);
    poll01!(0x0B, map_kpa,     decode_map);
    poll01!(0x0C, rpm,         decode_rpm);
    poll01!(0x0D, vss_kph,     decode_vss);
    poll01!(0x0E, spark_adv_deg, decode_spark_adv);
    poll01!(0x0F, iat_c,       decode_iat);
    poll01!(0x10, maf_gs,      decode_maf);
    poll01!(0x11, tps_pct,     decode_tps);

    // O2 sensors (Mode 01 — SWORD)
    if let Ok(r) = request_pid01(port, 0x14) {
        if let Some(v) = decode_o2_left_up(&r)  { d.o2_left_up_v  = v; }
    }
    if let Ok(r) = request_pid01(port, 0x15) {
        if let Some(v) = decode_o2_left_dn(&r)  { d.o2_left_dn_v  = v; }
    }
    if let Ok(r) = request_pid01(port, 0x18) {
        if let Some(v) = decode_o2_right_up(&r) { d.o2_right_up_v = v; }
    }
    if let Ok(r) = request_pid01(port, 0x19) {
        if let Some(v) = decode_o2_right_dn(&r) { d.o2_right_dn_v = v; }
    }

    // ── Mode 22 GM-specific PIDs ─────────────────────────────────────────
    macro_rules! poll22 {
        ($pid:expr, $field:ident, $decoder:expr) => {
            if let Ok(r) = request_pid22(port, $pid) {
                if let Some(v) = $decoder(&r) { d.$field = v; }
            }
        };
    }
    macro_rules! poll22b {
        ($pid:expr, $field:ident, $decoder:expr) => {
            if let Ok(r) = request_pid22(port, $pid) {
                if let Some(&b) = r.first() { d.$field = $decoder(b); }
            }
        };
    }

    poll22!(0x1140, maf_hi_gs,     decode_maf_hi);
    poll22!(0x1141, batt_volt,     decode_batt_volt);
    poll22!(0x1142, map_volts,     decode_map_volts);
    poll22!(0x1145, o2_lf_mv,      decode_o2_lf_mv);
    poll22!(0x1146, o2_lr_mv,      decode_o2_lr_mv);
    poll22!(0x1148, o2_rf_mv,      decode_o2_rf_mv);
    poll22!(0x1149, o2_rr_mv,      decode_o2_rr_mv);
    poll22!(0x1151, ect_c,         decode_accel_pedal);  // reuse field — override if needed
    poll22!(0x1170, evap_purge_pct,decode_evap_purge);
    poll22!(0x1176, iac_learned,   decode_iac_learned);
    poll22!(0x1179, iac_current,   decode_iac_current);
    poll22!(0x1190, fuel_trim_cell,decode_fuel_trim_cell);
    poll22!(0x1192, idle_desired,  decode_desired_idle);
    poll22!(0x119D, baro_kpa,      decode_baro);
    poll22!(0x119E, target_afr,    decode_target_afr);
    poll22!(0x119F, oil_life_pct,  decode_oil_life);
    poll22!(0x11A1, engine_run_min,decode_run_time_min);
    poll22!(0x11A3, cat_temp_c,    decode_cat_temp);
    poll22!(0x11A6, knock_retard,  decode_knock_retard);
    poll22!(0x116F, startup_ect_c, decode_startup_ect);
    poll22!(0x115C, oil_press_kpa, decode_oil_press_kpa);
    poll22!(0x125A, inj_pw_b1_ms,  decode_inj_pw);
    poll22!(0x125B, inj_pw_b2_ms,  decode_inj_pw);

    // Misfire
    poll22!(0x11EA, misfire_c5, decode_misfire);
    poll22!(0x11EB, misfire_c6, decode_misfire);
    poll22!(0x11EC, misfire_c7, decode_misfire);
    poll22!(0x11ED, misfire_c8, decode_misfire);

    // Transmission
    poll22!(0x19F3, trans_oil_temp, decode_trans_oil_temp);
    poll22!(0x19F4, trans_ratio,    decode_trans_ratio);
    poll22!(0x19F5, trans_gear,     decode_trans_gear);
    poll22!(0x199A, current_gear,   decode_current_gear);

    // Bitmapped registers
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
            d.vtd_fuel_dis = decode_vtd_fuel_disable(b);
            d.tcc_solenoid = decode_tcc_solenoid(b);
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

    // Derived channels
    if let Some(load) = calc_load_gcyl(d.maf_gs, d.rpm) {
        // stored in EcuTelemetry — future field: d.load_gcyl = load;
        let _ = load;
    }
    d.idc_b1_pct = calc_idc_b1(d.rpm, d.inj_pw_b1_ms);

    Ok(d)
}

// ─────────────────────────────────────────────────────────────────────────────
// App entry point
// ─────────────────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            port: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_serial_ports,
            connect_ecu,
            disconnect_ecu,
            connection_status,
            write_ecu_frame,
            read_ecu_frame,
            read_ecu_data,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri runtime error");
}
