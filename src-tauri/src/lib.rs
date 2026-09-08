// TuneItVerse lib.rs — Tauri entry + command surface (v3.11.0)
#![allow(unused_imports, dead_code, non_snake_case)]

mod a2l;
mod can;
mod checksum;
mod checksum_sizes;
mod consult;
mod cs_guard;
mod dtc;
mod ecu_database;
mod file_dialog;
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
mod transport;

use serialport::SerialPort;
use std::sync::Mutex;
use std::time::Duration;
use serde_json::json;
use std::collections::HashMap;

pub(crate) struct AppState {
    port: Option<Box<dyn SerialPort + Send>>,
    protocol: String,
    last_os_id: Option<String>,
    last_family: Option<String>,
}

pub(crate) static STATE: Mutex<AppState> = Mutex::new(AppState { port: None, protocol: String::new(), last_os_id: None, last_family: None });

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
    if crate::j2534::is_device_open() {
        return Ok("Connected (j2534)".into());
    }
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
    let proto_l = protocol.to_ascii_lowercase();
    if proto_l.contains("can") || proto_l.contains("uds") {
        let _ = crate::can::elm_init_can_500k(&mut port);
    } else if proto_l.contains("consult") {
        let _ = crate::consult::consult_init(&mut port);
    } else if proto_l.contains("kwp") {
        let _ = crate::kwp::kwp_fast_init(&mut port);
    }
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = Some(port);
    guard.protocol = protocol.clone();
    Ok(format!("Connected to {} @ {} baud ({})", port_name, baud, protocol))
}
#[tauri::command]
fn disconnect_ecu() -> Result<String, String> {
    let _ = crate::j2534::j2534_disconnect();
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
fn list_ecu_catalog() -> Result<String, String> {
    let rows: Vec<serde_json::Value> = ecu_database::load_ecu_database()
        .into_iter()
        .map(|e| json!({
            "ecu_family": e.ecu_family,
            "display_name": e.display_name,
            "protocol": e.protocol,
            "bin_size_bytes": e.bin_size_bytes,
            "hardware": e.hardware,
            "vehicles": e.vehicles,
            "checksum": e.checksum.r#type,
            "security": e.security_access.r#type,
        }))
        .collect();
    Ok(json!(rows).to_string())
}
#[tauri::command]
fn get_ecu_info(family_or_os: String) -> Result<String, String> {
    if let Some(e) = ecu_database::get_ecu_by_os_id(&family_or_os).or_else(|| ecu_database::get_ecu_by_family(&family_or_os)) {
        Ok(serde_json::to_string_pretty(&e).unwrap_or_else(|_| "{}".into()))
    } else {
        Ok(json!({"ecu_family": family_or_os, "display_name": "Unknown / not in DB"}).to_string())
    }
}

fn pull_mode01(port: &mut Box<dyn SerialPort + Send>, pid: u8) -> Option<Vec<u8>> {
    crate::transport::pull_mode01(port, pid)
}

#[tauri::command]
fn read_properties() -> Result<String, String> {
    let protocol = STATE.lock().map(|g| g.protocol.clone()).unwrap_or_default();
    let inner = with_port(|port| {
        let mut vin = "UNREAD".to_string();
        let mut calid = "UNREAD".to_string();
        if let Some(parsed) = crate::transport::pull_mode09(port, 0x02) {
            if parsed.len() >= 8 { vin = parsed; }
        }
        if let Some(parsed) = crate::transport::pull_mode09(port, 0x04) {
            if !parsed.is_empty() { calid = parsed; }
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
    with_port(|port| Ok(serde_json::Value::Object(crate::transport::decode_live_map(port)).to_string()))
        .or_else(|_| Ok(json!({"source":"offline","pids_decoded":0,"honest":true,"note":"Offline — no invented live PIDs."}).to_string()))
}
