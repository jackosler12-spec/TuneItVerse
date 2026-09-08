//! Protocol-aware OBD transport.
//!
//! Cheap ELM327 adapters speak ASCII (`010C\\r` → `41 0C ..`).
//! Raw VPW adapters want a J1850 header + checksum.
//! Sending VPW bytes down an ELM port is why live data looked dead.

use serialport::SerialPort;
use std::time::{Duration, Instant};

use crate::pid_decode::*;
use crate::vpw::{
    ascii_from_obd_payload, build_mode09_request, build_obd_request, parse_mode01_response,
    parse_mode09_response, request_response,
};

pub fn current_protocol() -> String {
    crate::STATE
        .lock()
        .map(|g| g.protocol.clone())
        .unwrap_or_default()
}

pub fn is_elm_ascii(protocol: &str) -> bool {
    let p = protocol.to_ascii_lowercase();
    p.contains("elm")
        || p.contains("auto")
        || p.contains("can")
        || p.contains("uds")
        || p.contains("kwp")
        || p.contains("consult")
        || p.is_empty()
}

pub fn is_raw_vpw(protocol: &str) -> bool {
    let p = protocol.to_ascii_lowercase();
    p.contains("vpw") || p.contains("j1850")
}

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
                if !collected.is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(8));
            }
            Err(_) => break,
        }
    }
    collected
}

pub fn elm_command(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    let _ = port.clear(serialport::ClearBuffer::Input);
    port.write_all(format!("{}\r", cmd).as_bytes())
        .map_err(|e| format!("ELM write {cmd}: {e}"))?;
    port.flush().ok();
    let raw = read_ascii(port, 400);
    let up = raw.to_ascii_uppercase();
    if up.contains("UNABLE") || up.contains("STOPPED") || up.contains("ERROR") {
        return Err(format!("ELM {cmd}: {}", raw.trim()));
    }
    Ok(raw)
}

pub fn parse_elm_hex(raw: &str) -> Vec<u8> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    let mut out = Vec::new();
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i..i + 2]).unwrap_or(""), 16) {
            out.push(b);
        }
        i += 2;
    }
    out
}

/// Pull Mode 01 PID payload using whichever transport the session opened.
pub fn pull_mode01(port: &mut Box<dyn SerialPort + Send>, pid: u8) -> Option<Vec<u8>> {
    let proto = current_protocol();
    if is_raw_vpw(&proto) && !is_elm_ascii(&proto) {
        return request_response(port, &build_obd_request(pid))
            .ok()
            .and_then(|resp| parse_mode01_response(&resp, pid));
    }
    let cmd = format!("01{:02X}", pid);
    let raw = elm_command(port, &cmd).ok()?;
    let bytes = parse_elm_hex(&raw);
    if let Some(data) = parse_mode01_response(&bytes, pid) {
        return Some(data);
    }
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == 0x41 && bytes[i + 1] == pid {
            return Some(bytes[i + 2..].to_vec());
        }
    }
    None
}

pub fn pull_mode09(port: &mut Box<dyn SerialPort + Send>, info: u8) -> Option<String> {
    let proto = current_protocol();
    if is_raw_vpw(&proto) && !is_elm_ascii(&proto) {
        return request_response(port, &build_mode09_request(info))
            .ok()
            .and_then(|resp| parse_mode09_response(&resp, info))
            .map(|d| ascii_from_obd_payload(&d))
            .filter(|s| !s.is_empty());
    }
    let cmd = format!("09{:02X}", info);
    let raw = elm_command(port, &cmd).ok()?;
    let bytes = parse_elm_hex(&raw);
    if let Some(data) = parse_mode09_response(&bytes, info) {
        let s = ascii_from_obd_payload(&data);
        if !s.is_empty() {
            return Some(s);
        }
    }
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == 0x49 && bytes[i + 1] == info {
            let s = ascii_from_obd_payload(&bytes[i + 2..]);
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

pub fn elm_service(port: &mut Box<dyn SerialPort + Send>, sid: u8) -> Result<Vec<u8>, String> {
    let raw = elm_command(port, &format!("{:02X}", sid))?;
    Ok(parse_elm_hex(&raw))
}

pub fn decode_live_map(port: &mut Box<dyn SerialPort + Send>) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    let mut decoded = 0u32;
    let mut put = |k: &str, v: Option<f32>, map: &mut serde_json::Map<String, serde_json::Value>, n: &mut u32| {
        if let Some(val) = v {
            map.insert(k.to_string(), serde_json::json!(val));
            *n += 1;
        }
    };
    put("rpm", pull_mode01(port, 0x0C).and_then(|d| decode_engine_rpm(&d)), &mut obj, &mut decoded);
    put("map", pull_mode01(port, 0x0B).and_then(|d| decode_map(&d)), &mut obj, &mut decoded);
    put("ect", pull_mode01(port, 0x05).and_then(|d| decode_ect(&d)), &mut obj, &mut decoded);
    put("tps", pull_mode01(port, 0x11).and_then(|d| decode_throttle_pos(&d)), &mut obj, &mut decoded);
    put("iat", pull_mode01(port, 0x0F).and_then(|d| decode_iat(&d)), &mut obj, &mut decoded);
    put("spark", pull_mode01(port, 0x0E).and_then(|d| decode_timing_advance(&d)), &mut obj, &mut decoded);
    put("batt", crate::flash::read_battery_voltage(port), &mut obj, &mut decoded);
    put("stft", pull_mode01(port, 0x06).and_then(|d| decode_stft_bank1(&d)), &mut obj, &mut decoded);
    put("ltft", pull_mode01(port, 0x07).and_then(|d| decode_ltft_bank1(&d)), &mut obj, &mut decoded);
    put("maf", pull_mode01(port, 0x10).and_then(|d| decode_maf_obd(&d)), &mut obj, &mut decoded);
    put("vss", pull_mode01(port, 0x0D).and_then(|d| decode_vss(&d)), &mut obj, &mut decoded);
    put("load", pull_mode01(port, 0x04).and_then(|d| decode_engine_load(&d)), &mut obj, &mut decoded);
    put("o2b1s1", pull_mode01(port, 0x14).and_then(|d| decode_o2_b1s1_obd(&d)), &mut obj, &mut decoded);
    put("o2b1s2", pull_mode01(port, 0x15).and_then(|d| decode_o2_b1s2_obd(&d)), &mut obj, &mut decoded);
    put("baro", pull_mode01(port, 0x33).and_then(|d| d.first().map(|&b| b as f32)), &mut obj, &mut decoded);
    put("fuel_status", pull_mode01(port, 0x03).and_then(|d| decode_fuel_system_status(&d)), &mut obj, &mut decoded);
    put("fuel_level", pull_mode01(port, 0x2F).and_then(|d| decode_fuel_level(&d)), &mut obj, &mut decoded);
    obj.insert("pids_decoded".into(), serde_json::json!(decoded));
    obj.insert(
        "source".into(),
        serde_json::json!(if decoded > 0 { "live-Mode01" } else { "live-empty" }),
    );
    obj.insert("honest".into(), serde_json::json!(true));
    obj.insert("transport".into(), serde_json::json!(current_protocol()));
    obj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elm_hex_strips_spaces_and_prompt() {
        let b = parse_elm_hex("41 0C 1A F8\r>");
        assert_eq!(b, vec![0x41, 0x0C, 0x1A, 0xF8]);
    }

    #[test]
    fn protocol_classifiers() {
        assert!(is_elm_ascii("can"));
        assert!(is_elm_ascii("uds"));
        assert!(is_elm_ascii("elm-auto (Mode 01 seen)"));
        assert!(is_raw_vpw("vpw"));
        assert!(!is_raw_vpw("can"));
    }
}
