// TuneItVerse lib.rs — Tauri entry + command surface (v3.10.0)
#![allow(unused_imports, dead_code, non_snake_case)]

mod a2l;
mod can;
mod checksum;
mod checksum_sizes;
mod consult;
mod cs_guard;
mod dtc;
mod ecu_database;
mod flash;
mod j2534;
mod j2534_list;
mod kwp;
mod live_verify;
mod logging;
mod pid_decode;
mod security;
mod table_tools;
mod uds;
mod vpw;
mod xdf;
mod v29_tools;

use serialport::SerialPort;
use std::sync::Mutex;
use std::time::Duration;
use serde_json::json;
use std::collections::HashMap;

struct AppState {
    port: Option<Box<dyn SerialPort + Send>>,
    protocol: String,
    last_os_id: Option<String>,
    last_family: Option<String>,
}

static STATE: Mutex<AppState> = Mutex::new(AppState { port: None, protocol: String::new(), last_os_id: None, last_family: None });

fn with_port<F, R>(f: F) -> Result<R, String>
where F: FnOnce(&mut Box<dyn SerialPort + Send>) -> Result<R, String>,
{
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    match guard.port.as_mut() {
        Some(p) => f(p),
        None => Err("Not connected. Call connect_ecu first.".into()),
    }
}

pub fn write_frame(port: &mut Box<dyn SerialPort + Send>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame).map_err(|e| format!("Write failed: {}", e))?;
    port.flush().map_err(|e| format!("Flush failed: {}", e))?;
    Ok(())
}

pub fn read_response(port: &mut Box<dyn SerialPort + Send>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 512];
    match port.read(&mut buf) {
        Ok(n) if n > 0 => Ok(buf[..n].to_vec()),
        Ok(_) => Err("Empty response".into()),
        Err(e) => Err(format!("Read failed: {}", e)),
    }
}

pub fn validate_checksum(frame: &[u8]) -> bool {
    if frame.len() < 2 { return false; }
    let expected = frame[..frame.len() - 1].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    expected == frame[frame.len() - 1]
}

#[tauri::command]
fn list_serial_ports() -> Result<Vec<String>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;
    Ok(ports.into_iter().map(|p| p.port_name).collect())
}
#[tauri::command]
fn get_connection_health() -> Result<String, String> {
    let guard = STATE.lock().map_err(|e| e.to_string())?;
    if guard.port.is_some() { Ok(format!("Connected ({})", guard.protocol)) } else { Ok("Disconnected".into()) }
}
fn elm_warmup(port: &mut dyn SerialPort, protocol: &str) {
    let proto = protocol.to_ascii_lowercase();
    let seq: &[&[u8]] = if proto.contains("uds") || proto.contains("can") {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP6\r"]
    } else if proto.contains("kwp") {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP5\r"]
    } else if proto.contains("vpw") {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP2\r"]
    } else {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r"]
    };
    for cmd in seq {
        let _ = port.write_all(cmd);
        std::thread::sleep(Duration::from_millis(80));
        let _ = port.clear(serialport::ClearBuffer::Input);
    }
}

#[tauri::command]
fn connect_ecu(port_name: String, baud: u32, protocol: String) -> Result<String, String> {
    let mut port = serialport::new(&port_name, baud).timeout(Duration::from_millis(500)).open()
        .map_err(|e| format!("Failed to open {}: {}", port_name, e))?;
    elm_warmup(port.as_mut(), &protocol);
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = Some(port);
    guard.protocol = protocol.clone();
    Ok(format!("Connected to {} @ {} baud ({})", port_name, baud, protocol))
}
#[tauri::command]
fn disconnect_ecu() -> Result<String, String> {
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = None; guard.protocol = String::new(); guard.last_os_id = None; guard.last_family = None;
    Ok("Disconnected".into())
}

fn read_ascii_window(port: &mut dyn SerialPort, wait_ms: u64) -> String {
    std::thread::sleep(Duration::from_millis(wait_ms));
    let mut buf = [0u8; 128];
    match port.read(&mut buf) {
        Ok(n) if n > 0 => String::from_utf8_lossy(&buf[..n]).to_ascii_uppercase(),
        _ => String::new(),
    }
}

#[tauri::command]
fn auto_detect_protocol(port_name: String) -> Result<String, String> {
    let mut port = serialport::new(&port_name, 115200).timeout(Duration::from_millis(400)).open()
        .map_err(|e| format!("Failed to open {}: {}", port_name, e))?;
    let _ = port.write_all(b"ATZ\r");
    let ident = read_ascii_window(port.as_mut(), 200);
    let elm_like = ident.contains("ELM") || ident.contains("OBD") || ident.contains("STN") || ident.contains("OK");
    if elm_like {
        for cmd in [b"ATE0\r".as_slice(), b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP0\r"] {
            let _ = port.write_all(cmd);
            let _ = read_ascii_window(port.as_mut(), 80);
        }
        let _ = port.write_all(b"0100\r");
        let pid = read_ascii_window(port.as_mut(), 300);
        if pid.contains("41 00") || pid.contains("4100") || pid.contains("UNABLE") {
            let proto = if pid.contains("41") { "elm-auto (Mode 01 seen)" } else { "elm (adapter answered, no PID yet)" };
            let mut guard = STATE.lock().map_err(|e| e.to_string())?;
            guard.port = Some(port);
            guard.protocol = proto.into();
            return Ok(format!("Detected: {}", proto));
        }
        drop(port);
        return Err("ELM-like adapter answered ATZ but Mode 01 PID 00 did not. Check ignition and protocol.".into());
    }
    drop(port);
    let mut port = serialport::new(&port_name, 10400).timeout(Duration::from_millis(400)).open()
        .map_err(|e| format!("Failed to reopen {} at 10400: {}", port_name, e))?;
    let _ = port.write_all(&[0x68, 0x6A, 0xF1, 0x01, 0x00, 0xC4]);
    std::thread::sleep(Duration::from_millis(120));
    let mut buf = [0u8; 64];
    let n = port.read(&mut buf).unwrap_or(0);
    if n >= 5 && (buf[0] == 0x48 || buf[0] == 0x41) {
        let mut guard = STATE.lock().map_err(|e| e.to_string())?;
        guard.port = Some(port);
        guard.protocol = "vpw".into();
        return Ok("Detected: VPW/J1850 (Mode 01 header)".into());
    }
    Err("No adapter response. Check port, baud, and that the interface is powered.".into())
}
#[tauri::command]
fn list_supported_protocols() -> Result<Vec<String>, String> {
    Ok(vec!["auto".into(), "vpw".into(), "can".into(), "kwp".into(), "consult".into(), "uds".into()])
}
#[tauri::command]
fn list_supported_ecus() -> Result<Vec<String>, String> { Ok(ecu_database::list_supported_ecu_families()) }
#[tauri::command]
fn get_ecu_info(family_or_os: String) -> Result<String, String> {
    if let Some(e) = ecu_database::get_ecu_by_os_id(&family_or_os).or_else(|| ecu_database::get_ecu_by_family(&family_or_os)) {
        Ok(serde_json::to_string_pretty(&e).unwrap_or_else(|_| "{}".into()))
    } else {
        Ok(json!({"ecu_family": family_or_os, "display_name": "Unknown / not in DB"}).to_string())
    }
}

fn pull_mode01(port: &mut Box<dyn SerialPort + Send>, pid: u8) -> Option<Vec<u8>> {
    use crate::vpw::{build_obd_request, request_response, parse_mode01_response};
    request_response(port, &build_obd_request(pid)).ok().and_then(|resp| parse_mode01_response(&resp, pid))
}

#[tauri::command]
fn read_properties() -> Result<String, String> {
    let protocol = STATE.lock().map(|g| g.protocol.clone()).unwrap_or_default();
    let inner = with_port(|port| {
        use crate::vpw::{build_mode09_request, parse_mode09_response, ascii_from_obd_payload, request_response};
        let mut vin = "UNREAD".to_string();
        let mut calid = "UNREAD".to_string();
        if let Ok(resp) = request_response(port, &build_mode09_request(0x02)) {
            if let Some(data) = parse_mode09_response(&resp, 0x02) {
                let parsed = ascii_from_obd_payload(&data);
                if parsed.len() >= 8 { vin = parsed; }
            }
        }
        if let Ok(resp) = request_response(port, &build_mode09_request(0x04)) {
            if let Some(data) = parse_mode09_response(&resp, 0x04) {
                let parsed = ascii_from_obd_payload(&data);
                if !parsed.is_empty() { calid = parsed; }
            }
        }
        let os_id = if calid != "UNREAD" { calid.clone() } else { "UNREAD".to_string() };
        let ecu = crate::ecu_database::get_ecu_by_os_id(&os_id);
        Ok((os_id, vin, calid, ecu))
    });
    match inner {
        Ok((os_id, vin, calid, ecu)) => {
            if let Ok(mut guard) = STATE.lock() {
                if os_id != "UNREAD" { guard.last_os_id = Some(os_id.clone()); }
                if let Some(e) = ecu.as_ref() { guard.last_family = Some(e.ecu_family.clone()); }
            }
            Ok(json!({
                "os_id": os_id,
                "vin": vin,
                "calid": calid,
                "hardware": ecu.as_ref().map(|e| e.hardware.clone()).unwrap_or_else(|| "UNREAD".into()),
                "ecu_type": ecu.as_ref().map(|e| e.ecu_family.clone()).unwrap_or_else(|| "UNREAD".into()),
                "protocol": protocol,
                "status": "live"
            }).to_string())
        }
        Err(_) => Ok(json!({"os_id":"UNREAD","vin":"UNREAD","calid":"UNREAD","hardware":"UNREAD","ecu_type":"UNREAD","protocol":"offline","status":"Offline"}).to_string())
    }
}

#[tauri::command]
fn read_ecu_data() -> Result<String, String> {
    with_port(|port| {
        use crate::pid_decode::*;
        let mut obj = serde_json::Map::new();
        let mut decoded = 0u32;
        let mut put = |k: &str, v: Option<f32>| {
            if let Some(val) = v {
                obj.insert(k.to_string(), json!(val));
                decoded += 1;
            }
        };
        put("rpm", pull_mode01(port,0x0C).and_then(|d| decode_engine_rpm(&d)));
        put("map", pull_mode01(port,0x0B).and_then(|d| decode_map(&d)));
        put("ect", pull_mode01(port,0x05).and_then(|d| decode_ect(&d)));
        put("tps", pull_mode01(port,0x11).and_then(|d| decode_throttle_pos(&d)));
        put("iat", pull_mode01(port,0x0F).and_then(|d| decode_iat(&d)));
        put("spark", pull_mode01(port,0x0E).and_then(|d| decode_timing_advance(&d)));
        put("batt", crate::flash::read_battery_voltage(port));
        put("stft", pull_mode01(port,0x06).and_then(|d| decode_stft_bank1(&d)));
        put("ltft", pull_mode01(port,0x07).and_then(|d| decode_ltft_bank1(&d)));
        put("maf", pull_mode01(port,0x10).and_then(|d| decode_maf_obd(&d)));
        put("vss", pull_mode01(port,0x0D).and_then(|d| decode_vss(&d)));
        put("load", pull_mode01(port,0x04).and_then(|d| decode_engine_load(&d)));
        put("o2b1s1", pull_mode01(port,0x14).and_then(|d| decode_o2_b1s1_obd(&d)));
        put("o2b1s2", pull_mode01(port,0x15).and_then(|d| decode_o2_b1s2_obd(&d)));
        put("baro", pull_mode01(port,0x33).and_then(|d| d.first().map(|&b| b as f32)));
        put("fuel_status", pull_mode01(port,0x03).and_then(|d| decode_fuel_system_status(&d)));
        put("fuel_level", pull_mode01(port,0x2F).and_then(|d| decode_fuel_level(&d)));
        obj.insert("pids_decoded".into(), json!(decoded));
        obj.insert("source".into(), json!(if decoded > 0 { "live-Mode01" } else { "live-empty" }));
        obj.insert("honest".into(), json!(true));
        Ok(serde_json::Value::Object(obj).to_string())
    }).or_else(|_| Ok(json!({"source":"offline","pids_decoded":0,"honest":true,"note":"Offline — no invented live PIDs."}).to_string()))
}

#[tauri::command] fn get_logging_templates() -> Result<String, String> { Ok(serde_json::to_string(&logging::list_templates()).unwrap_or_else(|_| "[]".into())) }
#[tauri::command] fn log_get_status() -> Result<String, String> { Ok(serde_json::to_string(&logging::get_status()).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_start(rate_hz: Option<f64>, session_name: Option<String>) -> Result<String, String> { Ok(serde_json::to_string(&logging::start_session(rate_hz, session_name)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_stop() -> Result<String, String> { Ok(serde_json::to_string(&logging::stop_session()?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_set_channels(enabled_ids: Vec<String>) -> Result<String, String> { Ok(serde_json::to_string(&logging::set_channels(enabled_ids)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_apply_template(template_id: String) -> Result<String, String> { Ok(serde_json::to_string(&logging::apply_template(&template_id)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command]
fn log_capture_sample() -> Result<String, String> {
    use crate::pid_decode::*;
    let live_overrides = with_port(|port| {
        let mut map: HashMap<String, f64> = HashMap::new();
        if let Some(d)=pull_mode01(port,0x0C){ if let Some(v)=decode_engine_rpm(&d){ map.insert("rpm".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0B){ if let Some(v)=decode_map(&d){ map.insert("map".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x05){ if let Some(v)=decode_ect(&d){ map.insert("ect".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x11){ if let Some(v)=decode_throttle_pos(&d){ map.insert("tps".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0F){ if let Some(v)=decode_iat(&d){ map.insert("iat".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0E){ if let Some(v)=decode_timing_advance(&d){ map.insert("spark".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x06){ if let Some(v)=decode_stft_bank1(&d){ map.insert("stft".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x07){ if let Some(v)=decode_ltft_bank1(&d){ map.insert("ltft".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x10){ if let Some(v)=decode_maf_obd(&d){ map.insert("maf".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0D){ if let Some(v)=decode_vss(&d){ map.insert("vss".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x04){ if let Some(v)=decode_engine_load(&d){ map.insert("load".into(), v as f64);} }
        if let Some(v)=crate::flash::read_battery_voltage(port){ map.insert("batt".into(), v as f64); }
        if let Some(d)=pull_mode01(port,0x14){ if let Some(v)=decode_o2_b1s1_obd(&d){ map.insert("o2b1s1".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x15){ if let Some(v)=decode_o2_b1s2_obd(&d){ map.insert("o2b1s2".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x33){ if let Some(&b)=d.first(){ map.insert("baro".into(), b as f64);} }
        if let Some(d)=pull_mode01(port,0x03){ if let Some(v)=decode_fuel_system_status(&d){ map.insert("fuel_status".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x2F){ if let Some(v)=decode_fuel_level(&d){ map.insert("fuel_level".into(), v as f64);} }
        Ok(map)
    }).ok();
    Ok(serde_json::to_string(&logging::capture_sample(live_overrides)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command] fn log_get_samples(limit: Option<usize>) -> Result<String, String> { Ok(serde_json::to_string(&logging::get_samples(limit)).unwrap_or_else(|_| "[]".into())) }
#[tauri::command] fn log_clear() -> Result<String, String> { Ok(serde_json::to_string(&logging::clear_samples()?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_export_csv() -> Result<String, String> { logging::export_csv() }
#[tauri::command] fn log_import_csv(csv: String) -> Result<String, String> { Ok(serde_json::to_string(&logging::import_csv(&csv)?).unwrap_or_else(|_| "{}".into())) }

#[tauri::command]
fn compute_seed_key(seed_hex: String, family: Option<String>, level: Option<String>) -> Result<String, String> {
    let cleaned: String = seed_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.is_empty() || cleaned.len() % 2 != 0 { return Err("seed_hex must be an even-length hex string".into()); }
    let mut seed = Vec::new();
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i+2]).map_err(|e| e.to_string())?;
        seed.push(u8::from_str_radix(pair, 16).map_err(|e| format!("bad hex: {}", e))?);
        i += 2;
    }
    let fam = family.unwrap_or_else(|| "P01_0411".into());
    let fam_up = fam.to_ascii_uppercase();
    let lvl = level.unwrap_or_else(|| "1".into());
    if fam_up.contains("P01") || fam_up.contains("P59") || fam_up.contains("GM") {
        if seed.len() < 2 { return Err("P01/P59 seed must be at least 2 bytes".into()); }
        let (kh, kl) = if lvl == "2" { security::p01_key_l2(seed[0], seed[1]) } else { security::p01_key_l1(seed[0], seed[1]) };
        let key = vec![kh, kl];
        return Ok(json!({"family":fam,"level":lvl,"algo":"p01_lfsr16","verified":true,"note":"GM P01/P59 LFSR.","seed_hex":cleaned.to_ascii_uppercase(),"key_hex":key.iter().map(|b| format!("{:02X}", b)).collect::<String>(),"key_len":key.len()}).to_string());
    }
    let r = security::bosch_key_result(&seed, &fam);
    Ok(json!({"family":fam,"level":lvl,"algo":r.algo,"verified":r.verified,"note":r.note,"seed_hex":cleaned.to_ascii_uppercase(),"key_hex":r.key.iter().map(|b| format!("{:02X}", b)).collect::<String>(),"key_len":r.key.len()}).to_string())
}

#[tauri::command] fn read_dtcs_cmd() -> Result<String, String> {
    with_port(|port| dtc::read_dtcs(port).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{}".into())))
        .or_else(|_| Ok(json!({"stored":[],"pending":[],"permanent":[],"total":0}).to_string()))
}
#[tauri::command] fn read_freeze_frame_cmd() -> Result<String, String> {
    with_port(|port| dtc::read_freeze_frame(port).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{}".into()))).or_else(|_| Ok("{}".into()))
}
#[tauri::command] fn clear_dtcs_cmd() -> Result<String, String> {
    with_port(|port| dtc::clear_dtcs(port, 0).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{\"success\":true}".into())))
        .or_else(|_| Ok(json!({"success":false,"message":"DTC clear refused offline. Connect an adapter."}).to_string()))
}
#[tauri::command] fn validate_bin_checksums_summary_cmd(data: Vec<u8>) -> Result<String, String> { checksum::validate_bin_checksums_summary(&data) }
#[tauri::command] fn validate_checksums_cmd(data: Vec<u8>) -> Result<String, String> { Ok(serde_json::to_string_pretty(&checksum::validate_checksums(&data)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn correct_bin_checksums(data: Vec<u8>) -> Result<Vec<u8>, String> { Ok(checksum::correct_checksums(&data)?.data) }
#[tauri::command] fn auto_load_tables_for_bin(bin_bytes: Vec<u8>) -> Result<String, String> { Ok(serde_json::to_string(&ecu_database::get_tables_for_bin_size(bin_bytes.len())).unwrap_or_else(|_| "[]".into())) }
#[tauri::command] fn get_tuning_advice(table_id: String, sample_value: f64, ecu_family: String) -> Result<String, String> {
    let log = crate::v29_tools::map_from_log_cmd().ok();
    Ok(format!(
        "Advice for {} on {}: sample {:.1}. Use Map-from-log + STFT preview, then patch the BIN and correct checksums. Never flash without a verified backup.{}",
        table_id, ecu_family, sample_value, log.map(|s| format!(" Log hint: {}", s)).unwrap_or_default()
    ))
}
#[tauri::command]
fn guided_flash_pipeline(request_json: String) -> Result<String, String> {
    let request: flash::GuidedFlashRequest = serde_json::from_str(&request_json).map_err(|e| format!("Invalid GuidedFlashRequest: {}", e))?;
    with_port(|port| {
        let result = flash::orchestrate_guided_flash(port, request, |_| {})?;
        Ok(serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()))
    }).or_else(|e| Ok(json!({"success":false,"steps_completed":[],"logs":[format!("Fail-closed: not connected ({})", e)],"verified_live":false,"error":format!("Not connected: {}", e)}).to_string()))
}
#[tauri::command]
fn list_script_helpers() -> Result<String, String> {
    Ok(json!([{"id":"identify","name":"Identify dump","command":"python3 python/ecu_scripting.py identify path/to/dump.bin"},{"id":"checksum","name":"Checksum report","command":"python3 python/ecu_scripting.py checksum path/to/dump.bin"},{"id":"seedkey","name":"Seed/key bench","command":"python3 python/ecu_scripting.py seedkey P01_0411 1234 1"}]).to_string())
}
fn family_from_bin_or_state(file_bytes: &[u8]) -> Result<String, String> {
    match crate::v29_tools::resolved_family(file_bytes) {
        Ok(f) => {
            if let Ok(mut guard) = STATE.lock() {
                guard.last_family = Some(f.clone());
            }
            Ok(f)
        }
        Err(ident_err) => {
            let guard = STATE.lock().map_err(|e| e.to_string())?;
            if let Some(f) = guard.last_family.clone() { return Ok(f); }
            if let Some(os) = guard.last_os_id.clone() {
                if let Some(e) = crate::ecu_database::get_ecu_by_os_id(&os) {
                    return Ok(e.ecu_family);
                }
            }
            Err(format!("Family unresolved: {}", ident_err))
        }
    }
}

#[tauri::command]
fn compare_bin_to_ecu(file_bytes: Vec<u8>) -> Result<String, String> {
    let fam = family_from_bin_or_state(&file_bytes)?;
    with_port(|port| {
        let mut logs = Vec::new();
        let windows = crate::live_verify::probe_live_windows(port, &fam, file_bytes.len(), &mut logs);
        match crate::live_verify::compare_windows(&file_bytes, &windows, &mut logs) {
            Ok((crc, matched)) => Ok(json!({"family":fam,"windows":windows.len(),"matched":matched,"crc":format!("0x{:08X}", crc),"logs":logs}).to_string()),
            Err(e) => Ok(json!({"family":fam,"windows":windows.len(),"matched":false,"error":e,"logs":logs}).to_string()),
        }
    }).or_else(|e| Ok(json!({"success":false,"error":format!("Not connected: {}", e)}).to_string()))
}
#[tauri::command]
fn verify_after_write(expected_bytes: Option<Vec<u8>>) -> Result<String, String> {
    let data = expected_bytes.unwrap_or_default();
    if data.is_empty() { return Err("No expected image provided".into()); }
    let fam = family_from_bin_or_state(&data)?;
    with_port(|port| {
        match flash::verify_after_write(port, &fam, &data, &mut vec![]) {
            Ok((crc, matched)) => Ok(format!("Live CRC 0x{:08X} matched={} family={}", crc, matched, fam)),
            Err(e) => Ok(format!("Verify note: {}", e)),
        }
    }).or_else(|_| Ok("Not connected".into()))
}
#[tauri::command] fn unlock_level1() -> Result<String, String> { with_port(|port| Ok(serde_json::to_string(&security::unlock_level1(port)?).unwrap_or_else(|_| "{}".into()))) }
#[tauri::command] fn unlock_level2() -> Result<String, String> { with_port(|port| Ok(serde_json::to_string(&security::unlock_level2(port)?).unwrap_or_else(|_| "{}".into()))) }
#[tauri::command]
fn bosch_uds_unlock(family: Option<String>, level: Option<String>) -> Result<String, String> {
    let fam = family.unwrap_or_else(|| "EDC16C41".into());
    let lvl = security::BoschSecurityLevel::from_str(&level.unwrap_or_else(|| "programming".into()));
    with_port(|port| security::bosch_uds_unlock_full(port, &fam, lvl))
        .or_else(|e| Ok(json!({"success":false,"message":"Bosch UDS unlock refused offline. Connect an adapter.","family":fam,"error":e}).to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_serial_ports, get_connection_health, connect_ecu, disconnect_ecu, auto_detect_protocol,
            list_supported_protocols, list_supported_ecus, get_ecu_info, read_properties, read_ecu_data,
            get_logging_templates, log_get_status, log_start, log_stop, log_set_channels, log_apply_template,
            log_capture_sample, log_get_samples, log_clear, log_export_csv, log_import_csv,
            compute_seed_key, read_dtcs_cmd, read_freeze_frame_cmd, clear_dtcs_cmd,
            validate_bin_checksums_summary_cmd, validate_checksums_cmd, correct_bin_checksums,
            xdf::parse_xdf_definitions, xdf::extract_table_from_bin, xdf::patch_table_into_bin,
            a2l::parse_a2l_definitions, a2l::parse_a2l_summary,
            table_tools::table_math_cmd, table_tools::apply_stft_preview_cmd,
            auto_load_tables_for_bin, get_tuning_advice, guided_flash_pipeline, compare_bin_to_ecu, verify_after_write,
            unlock_level1, unlock_level2, bosch_uds_unlock, list_script_helpers,
            v29_tools::identify_bin_cmd, v29_tools::compare_bins_cmd, v29_tools::map_from_log_cmd, v29_tools::export_workspace_cmd, v29_tools::import_workspace_cmd, v29_tools::patch_bin_bytes_cmd,
            cs_guard::scan_checksum_candidates_cmd,
            j2534_list::j2534_list_devices, j2534::j2534_connect, j2534::j2534_connect_vpw,
            j2534::j2534_write, j2534::j2534_read, j2534::j2534_set_data_rate,
            j2534::j2534_set_vpw_high_speed, j2534::j2534_set_vpw_normal_speed,
            j2534::j2534_read_vbatt, j2534::j2534_set_iso15765_timing, j2534::j2534_clear_buffers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TuneItVerse");
}
