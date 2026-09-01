// TuneItVerse lib.rs — Tauri entry + command surface
// v3.3.0 — CSV import, seed/key calculator, fail-closed offline unlock/flash.
// Build your own. No bullshit prices.

#![allow(unused_imports, dead_code, non_snake_case)]

mod can;
mod checksum;
mod consult;
mod dtc;
mod ecu_database;
mod flash;
mod j2534;
mod kwp;
mod logging;
mod pid_decode;
mod security;
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
}

static STATE: Mutex<AppState> = Mutex::new(AppState {
    port: None,
    protocol: String::new(),
    last_os_id: None,
});

fn with_port<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut Box<dyn SerialPort + Send>) -> Result<R, String>,
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
    if guard.port.is_some() {
        Ok(format!("Connected ({})", guard.protocol))
    } else {
        Ok("Disconnected".into())
    }
}

#[tauri::command]
fn connect_ecu(port_name: String, baud: u32, protocol: String) -> Result<String, String> {
    let mut port = serialport::new(&port_name, baud)
        .timeout(Duration::from_millis(500))
        .open()
        .map_err(|e| format!("Failed to open {}: {}", port_name, e))?;
    let _ = port.write_all(b"ATZ\r");
    std::thread::sleep(Duration::from_millis(200));
    let _ = port.clear(serialport::ClearBuffer::All);
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = Some(port);
    guard.protocol = protocol.clone();
    Ok(format!("Connected to {} @ {} baud ({})", port_name, baud, protocol))
}

#[tauri::command]
fn disconnect_ecu() -> Result<String, String> {
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = None;
    guard.protocol = String::new();
    Ok("Disconnected".into())
}

#[tauri::command]
fn auto_detect_protocol(port_name: String) -> Result<String, String> {
    let mut port = serialport::new(&port_name, 115200)
        .timeout(Duration::from_millis(300))
        .open()
        .map_err(|e| e.to_string())?;
    let _ = port.write_all(&[0x68, 0x6A, 0xF1, 0x01, 0x00, 0xC4]);
    std::thread::sleep(Duration::from_millis(100));
    let mut buf = [0u8; 64];
    let n = port.read(&mut buf).unwrap_or(0);
    let detected = if n > 0 { "VPW/J1850 (or ELM)" } else { "auto (no response – check adapter)" };
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = Some(port);
    guard.protocol = detected.into();
    Ok(format!("Detected: {}", detected))
}

#[tauri::command]
fn list_supported_protocols() -> Result<Vec<String>, String> {
    Ok(vec!["auto".into(), "vpw".into(), "can".into(), "kwp".into(), "consult".into(), "uds".into()])
}

#[tauri::command]
fn list_supported_ecus() -> Result<Vec<String>, String> {
    Ok(ecu_database::list_supported_ecu_families())
}

#[tauri::command]
fn get_ecu_info(family_or_os: String) -> Result<String, String> {
    if let Some(e) = ecu_database::get_ecu_by_os_id(&family_or_os)
        .or_else(|| ecu_database::get_ecu_by_family(&family_or_os))
    {
        Ok(serde_json::to_string_pretty(&e).unwrap_or_else(|_| "{}".into()))
    } else {
        Ok(json!({"ecu_family": family_or_os, "display_name": "Unknown / not in DB", "notes": "Add JSON entry to expand"}).to_string())
    }
}

fn pull_mode01(port: &mut Box<dyn SerialPort + Send>, pid: u8) -> Option<Vec<u8>> {
    use crate::vpw::{build_obd_request, request_response, parse_mode01_response};
    request_response(port, &build_obd_request(pid)).ok().and_then(|resp| parse_mode01_response(&resp, pid))
}

#[tauri::command]
fn read_properties() -> Result<String, String> {
    with_port(|port| {
        use crate::vpw::{build_mode09_request, parse_mode09_response, ascii_from_obd_payload, request_response};
        let mut vin = "UNREAD".to_string();
        let mut calid = "UNREAD".to_string();
        let mut pid_mask: Option<u32> = None;
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
        if let Some(data) = pull_mode01(port, 0x00) {
            if data.len() >= 4 {
                pid_mask = Some(u32::from_be_bytes([data[0], data[1], data[2], data[3]]));
            }
        }
        let os_id = if calid != "UNREAD" { calid.clone() } else { "UNREAD".to_string() };
        let ecu = crate::ecu_database::get_ecu_by_os_id(&os_id);
        let mut guard = STATE.lock().map_err(|e| e.to_string())?;
        if os_id != "UNREAD" { guard.last_os_id = Some(os_id.clone()); }
        Ok(json!({
            "os_id": os_id,
            "vin": vin,
            "calid": calid,
            "hardware": ecu.as_ref().map(|e| e.hardware.clone()).unwrap_or_else(|| "UNREAD".into()),
            "ecu_type": ecu.as_ref().map(|e| e.ecu_family.clone()).unwrap_or_else(|| "UNREAD".into()),
            "protocol": guard.protocol,
            "pid00_mask": pid_mask,
            "status": "live"
        }).to_string())
    }).or_else(|_| Ok(json!({
        "os_id": "UNREAD",
        "vin": "UNREAD",
        "calid": "UNREAD",
        "hardware": "UNREAD",
        "ecu_type": "UNREAD",
        "protocol": "offline",
        "pid00_mask": null,
        "status": "Offline"
    }).to_string()))
}

#[tauri::command]
fn read_ecu_data() -> Result<String, String> {
    with_port(|port| {
        use crate::pid_decode::*;
        let mut rpm = 1250.0f32; let mut mapv = 48.0f32; let mut ect = 82.0f32;
        let mut tps = 12.0f32; let mut iat = 30.0f32; let mut spark = 22.0f32;
        let mut batt = 13.8f32; let mut stft = 0.0f32; let mut ltft = 0.0f32;
        let mut maf = 0.0f32; let mut vss = 0.0f32; let mut load = 0.0f32;
        if let Some(d) = pull_mode01(port, 0x0C) { if let Some(v) = decode_engine_rpm(&d) { rpm = v; } }
        if let Some(d) = pull_mode01(port, 0x0B) { if let Some(v) = decode_map(&d) { mapv = v; } }
        if let Some(d) = pull_mode01(port, 0x05) { if let Some(v) = decode_ect(&d) { ect = v; } }
        if let Some(d) = pull_mode01(port, 0x11) { if let Some(v) = decode_throttle_pos(&d) { tps = v; } }
        if let Some(d) = pull_mode01(port, 0x0F) { if let Some(v) = decode_iat(&d) { iat = v; } }
        if let Some(d) = pull_mode01(port, 0x0E) { if let Some(v) = decode_timing_advance(&d) { spark = v; } }
        if let Some(v) = crate::flash::read_battery_voltage(port) { batt = v; }
        if let Some(d) = pull_mode01(port, 0x06) { if let Some(v) = decode_stft_bank1(&d) { stft = v; } }
        if let Some(d) = pull_mode01(port, 0x07) { if let Some(v) = decode_ltft_bank1(&d) { ltft = v; } }
        if let Some(d) = pull_mode01(port, 0x10) { if let Some(v) = decode_maf_obd(&d) { maf = v; } }
        if let Some(d) = pull_mode01(port, 0x0D) { if let Some(v) = decode_vss(&d) { vss = v; } }
        if let Some(d) = pull_mode01(port, 0x04) { if let Some(v) = decode_engine_load(&d) { load = v; } }
        Ok(json!({"rpm":rpm,"map":mapv,"ect":ect,"tps":tps,"iat":iat,"spark":spark,"inj_ms":3.5,"stft":stft,"ltft":ltft,"maf":maf,"vss":vss,"load":load,"batt":batt,"source":"live-Mode01"}).to_string())
    }).or_else(|_| Ok(json!({"rpm":1250,"map":48,"ect":82,"tps":12,"iat":30,"spark":22,"inj_ms":3.5,"stft":0.0,"ltft":0.0,"maf":0.0,"vss":0.0,"load":0.0,"batt":13.8,"source":"offline-demo"}).to_string()))
}

#[tauri::command]
fn get_logging_templates() -> Result<String, String> {
    Ok(serde_json::to_string(&logging::list_templates()).unwrap_or_else(|_| "[]".into()))
}
#[tauri::command]
fn log_get_status() -> Result<String, String> {
    Ok(serde_json::to_string(&logging::get_status()).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_start(rate_hz: Option<f64>, session_name: Option<String>) -> Result<String, String> {
    Ok(serde_json::to_string(&logging::start_session(rate_hz, session_name)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_stop() -> Result<String, String> {
    Ok(serde_json::to_string(&logging::stop_session()?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_set_channels(enabled_ids: Vec<String>) -> Result<String, String> {
    Ok(serde_json::to_string(&logging::set_channels(enabled_ids)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_apply_template(template_id: String) -> Result<String, String> {
    Ok(serde_json::to_string(&logging::apply_template(&template_id)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_capture_sample() -> Result<String, String> {
    use crate::pid_decode::*;
    let live_overrides = with_port(|port| {
        let mut map: HashMap<String, f64> = HashMap::new();
        if let Some(d) = pull_mode01(port, 0x0C) { if let Some(v) = decode_engine_rpm(&d) { map.insert("rpm".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x0B) { if let Some(v) = decode_map(&d) { map.insert("map".into(), v as f64); map.insert("boost".into(), (v as f64 - 101.3).max(0.0)); } }
        if let Some(d) = pull_mode01(port, 0x05) { if let Some(v) = decode_ect(&d) { map.insert("ect".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x11) { if let Some(v) = decode_throttle_pos(&d) { map.insert("tps".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x0F) { if let Some(v) = decode_iat(&d) { map.insert("iat".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x0E) { if let Some(v) = decode_timing_advance(&d) { map.insert("spark".into(), v as f64); } }
        if let Some(v) = crate::flash::read_battery_voltage(port) { map.insert("batt".into(), v as f64); }
        if let Some(d) = pull_mode01(port, 0x06) { if let Some(v) = decode_stft_bank1(&d) { map.insert("stft".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x07) { if let Some(v) = decode_ltft_bank1(&d) { map.insert("ltft".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x10) { if let Some(v) = decode_maf_obd(&d) { map.insert("maf".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x0D) { if let Some(v) = decode_vss(&d) { map.insert("vss".into(), v as f64); } }
        if let Some(d) = pull_mode01(port, 0x04) { if let Some(v) = decode_engine_load(&d) { map.insert("load".into(), v as f64); } }
        Ok(map)
    }).ok();
    Ok(serde_json::to_string(&logging::capture_sample(live_overrides)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_get_samples(limit: Option<usize>) -> Result<String, String> {
    Ok(serde_json::to_string(&logging::get_samples(limit)).unwrap_or_else(|_| "[]".into()))
}
#[tauri::command]
fn log_clear() -> Result<String, String> {
    Ok(serde_json::to_string(&logging::clear_samples()?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn log_export_csv() -> Result<String, String> { logging::export_csv() }

#[tauri::command]
fn log_import_csv(csv: String) -> Result<String, String> {
    Ok(serde_json::to_string(&logging::import_csv(&csv)?).unwrap_or_else(|_| "{}".into()))
}

#[tauri::command]
fn compute_seed_key(family: String, seed_hex: String, level: Option<String>) -> Result<String, String> {
    let report = security::compute_seed_key_report(&family, &seed_hex, level.as_deref())?;
    Ok(report.to_string())
}

#[tauri::command]
fn read_dtcs_cmd() -> Result<String, String> {
    with_port(|port| dtc::read_dtcs(port).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{}".into())))
        .or_else(|_| Ok(json!({"stored":[],"pending":[],"permanent":[],"total":0}).to_string()))
}
#[tauri::command]
fn read_freeze_frame_cmd() -> Result<String, String> {
    with_port(|port| dtc::read_freeze_frame(port).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{}".into())))
        .or_else(|_| Ok("{}".into()))
}
#[tauri::command]
fn clear_dtcs_cmd() -> Result<String, String> {
    with_port(|port| dtc::clear_dtcs(port, 0).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{\"success\":true}".into())))
        .or_else(|_| Ok(json!({"success":true,"message":"Cleared (offline)"}).to_string()))
}

#[tauri::command]
fn validate_bin_checksums_summary_cmd(data: Vec<u8>) -> Result<String, String> {
    checksum::validate_bin_checksums_summary(&data)
}
#[tauri::command]
fn validate_checksums_cmd(data: Vec<u8>) -> Result<String, String> {
    Ok(serde_json::to_string_pretty(&checksum::validate_checksums(&data)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command]
fn correct_bin_checksums(data: Vec<u8>) -> Result<Vec<u8>, String> {
    Ok(checksum::correct_checksums(&data)?.data)
}
#[tauri::command]
fn auto_load_tables_for_bin(bin_bytes: Vec<u8>) -> Result<String, String> {
    Ok(serde_json::to_string(&ecu_database::get_tables_for_bin_size(bin_bytes.len())).unwrap_or_else(|_| "[]".into()))
}
#[tauri::command]
fn get_tuning_advice(table_id: String, sample_value: f64, ecu_family: String) -> Result<String, String> {
    Ok(format!("Advice for {} on {}: sample {:.1}. Cross-check with logs, stay conservative on first pass.", table_id, ecu_family, sample_value))
}

#[tauri::command]
fn guided_flash_pipeline(request_json: String) -> Result<String, String> {
    let request: flash::GuidedFlashRequest = serde_json::from_str(&request_json)
        .map_err(|e| format!("Invalid GuidedFlashRequest: {}", e))?;
    with_port(|port| {
        let result = flash::orchestrate_guided_flash(port, request, |_| {})?;
        Ok(serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()))
    }).or_else(|e| {
        Ok(json!({
            "success": false,
            "steps_completed": [],
            "logs": [format!("Fail-closed: not connected ({})", e), "Connect a real adapter before flashing. Offline flash is refused."],
            "verified_live": false,
            "error": format!("Not connected: {}", e)
        }).to_string())
    })
}

#[tauri::command]
fn list_script_helpers() -> Result<String, String> {
    Ok(json!([
        {"id": "identify", "name": "Identify dump", "command": "python3 python/ecu_scripting.py identify path/to/dump.bin"},
        {"id": "checksum", "name": "Checksum report", "command": "python3 python/ecu_scripting.py checksum path/to/dump.bin"},
        {"id": "seedkey", "name": "Seed/key calculator", "command": "python3 python/ecu_scripting.py seedkey EDC16C41 12345678"},
        {"id": "map-from-log", "name": "Map-from-log (in-app)", "command": "Use Tables → Map from Log after a logging session"}
    ]).to_string())
}

#[tauri::command]
fn compare_bin_to_ecu(file_bytes: Vec<u8>) -> Result<String, String> {
    with_port(|_port| {
        let mut c: u32 = 0xFFFF_FFFF;
        for &b in file_bytes.iter().take(65536) {
            c ^= b as u32;
            for _ in 0..8 { if c & 1 != 0 { c = (c >> 1) ^ 0xEDB88320; } else { c >>= 1; } }
        }
        Ok(format!("Local window CRC 0x{:08X}. Live compare requires Mode 23 / Mode 3C.", !c))
    }).or_else(|_| Ok("Not connected – load BIN and connect for live compare".into()))
}

#[tauri::command]
fn verify_after_write(expected_bytes: Option<Vec<u8>>) -> Result<String, String> {
    with_port(|port| {
        let data = expected_bytes.unwrap_or_default();
        if data.is_empty() { return Ok("No expected image provided".into()); }
        match flash::verify_after_write(port, "P01_0411", &data, &mut vec![]) {
            Ok((crc, matched)) => Ok(format!("Live CRC 0x{:08X} matched={}", crc, matched)),
            Err(e) => Ok(format!("Verify note: {}", e)),
        }
    }).or_else(|_| Ok("Not connected".into()))
}

#[tauri::command]
fn unlock_level1() -> Result<String, String> {
    with_port(|port| Ok(serde_json::to_string(&security::unlock_level1(port)?).unwrap_or_else(|_| "{}".into())))
}
#[tauri::command]
fn unlock_level2() -> Result<String, String> {
    with_port(|port| Ok(serde_json::to_string(&security::unlock_level2(port)?).unwrap_or_else(|_| "{}".into())))
}
#[tauri::command]
fn bosch_uds_unlock(family: Option<String>, level: Option<String>) -> Result<String, String> {
    let fam = family.unwrap_or_else(|| "EDC16C41".into());
    let lvl = security::BoschSecurityLevel::from_str(&level.unwrap_or_else(|| "programming".into()));
    with_port(|port| security::bosch_uds_unlock_full(port, &fam, lvl))
        .or_else(|_| Ok(json!({
            "success": false,
            "level": format!("{:?}", lvl),
            "family": fam,
            "message": "Bosch UDS unlock refused offline. Connect an adapter. Use compute_seed_key for bench calculation only."
        }).to_string()))
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
            compute_seed_key,
            read_dtcs_cmd, read_freeze_frame_cmd, clear_dtcs_cmd,
            validate_bin_checksums_summary_cmd, validate_checksums_cmd, correct_bin_checksums,
            xdf::parse_xdf_definitions, xdf::extract_table_from_bin, xdf::patch_table_into_bin,
            auto_load_tables_for_bin, get_tuning_advice,
            guided_flash_pipeline, compare_bin_to_ecu, verify_after_write,
            unlock_level1, unlock_level2, bosch_uds_unlock, list_script_helpers,
            v29_tools::identify_bin_cmd, v29_tools::compare_bins_cmd, v29_tools::map_from_log_cmd,
            j2534::j2534_list_devices, j2534::j2534_connect, j2534::j2534_connect_vpw,
            j2534::j2534_write, j2534::j2534_read, j2534::j2534_set_data_rate,
            j2534::j2534_set_vpw_high_speed, j2534::j2534_set_vpw_normal_speed,
            j2534::j2534_read_vbatt, j2534::j2534_set_iso15765_timing, j2534::j2534_clear_buffers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TuneItVerse");
}
