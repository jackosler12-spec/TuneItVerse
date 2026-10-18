//! Protocol-aware OBD transport.
//!
//! Cheap ELM327 adapters speak ASCII (`010C\r` → `41 0C ..`).
//! Raw VPW adapters want a J1850 header + checksum.
//! OBDLink / STN devices support the full ELM AT set plus the richer ST command set
//! (see OBDLink Family Reference and Programming Manual).
//!
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

/// Send an AT or ST command and wait for the prompt.
/// Returns the raw response (trimmed). Errors on UNABLE / ERROR / STOPPED.
pub fn elm_command(port: &mut Box<dyn SerialPort + Send>, cmd: &str) -> Result<String, String> {
    let _ = port.clear(serialport::ClearBuffer::Input);
    port.write_all(format!("{}\r", cmd).as_bytes())
        .map_err(|e| format!("ELM/ST write {cmd}: {e}"))?;
    port.flush().ok();
    let raw = read_ascii(port, 400);
    let up = raw.to_ascii_uppercase();
    if up.contains("UNABLE") || up.contains("STOPPED") || up.contains("ERROR") {
        return Err(format!("ELM/ST {cmd}: {}", raw.trim()));
    }
    Ok(raw)
}

/// Preferred adapter voltage read (OBDLink STVR). Falls back to ATRV.
/// Returns calibrated volts when the adapter supports it.
pub fn read_adapter_voltage(port: &mut Box<dyn SerialPort + Send>) -> Option<f32> {
    // STVR is the recommended command per FRPM (ATRV is deprecated).
    if let Ok(raw) = elm_command(port, "STVR") {
        if let Some(v) = crate::voltage_watch::parse_at_rv(&raw) {
            return Some(v);
        }
    }
    // Fallback for pure ELM327 clones.
    if let Ok(raw) = elm_command(port, "ATRV") {
        return crate::voltage_watch::parse_at_rv(&raw);
    }
    None
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
    let raw = elm_command(port, &cmd)?;
    Ok(parse_elm_hex(&raw))
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
    // Short timeout for interactive use; callers can raise for flash.
    let _ = set_timeout_ms(port, 100);
    read_adapter_voltage(port).ok_or_else(|| "No STVR/ATRV after setup".to_string())
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
    // Prefer adapter voltage (STVR) over Mode 01 PID 0x42 when on ELM path.
    put(
        "batt",
        read_adapter_voltage(port).or_else(|| crate::flash::read_battery_voltage(port)),
        &mut obj,
        &mut decoded,
    );
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
}
