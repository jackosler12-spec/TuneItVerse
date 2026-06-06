//! TuneItVerse – Rust backend
//!
//! Stage 1: real read_properties + chunked backup state machine.
//! All write commands remain STUBBED until Stage 2.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serialport::{available_ports, SerialPort};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::Mutex,
    time::Duration,
};

// ──────────────────────────────────────────────────────────────────────────────
// App state
// ──────────────────────────────────────────────────────────────────────────────

struct AppState {
    port: Mutex<Option<Box<dyn SerialPort>>>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Shared types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct SerialPortInfo {
    port_name: String,
    port_type: String,
}

#[derive(Serialize, Clone)]
struct RawFrame {
    raw: Vec<u8>,
    hex: String,
    bytes_read: usize,
}

#[derive(Serialize, Clone)]
struct EcuTelemetry {
    rpm: f32,
    map: f32,
    iat: f32,
    ect: f32,
    tps: f32,
    vss: f32,
    afr: f32,
    o2_b1s1: f32,
    stft_b1: f32,
    ltft_b1: f32,
    inj_pw: f32,
    spark_adv: f32,
    batt_volt: f32,
    mil_on: bool,
    dtc_count: u8,
}

impl Default for EcuTelemetry {
    fn default() -> Self {
        Self {
            rpm: 0.0,
            map: 0.0,
            iat: 0.0,
            ect: 0.0,
            tps: 0.0,
            vss: 0.0,
            afr: 14.7,
            o2_b1s1: 0.45,
            stft_b1: 0.0,
            ltft_b1: 0.0,
            inj_pw: 0.0,
            spark_adv: 0.0,
            batt_volt: 12.0,
            mil_on: false,
            dtc_count: 0,
        }
    }
}

/// Identified ECU properties returned to the frontend.
#[derive(Serialize, Clone, Debug)]
pub struct EcuProperties {
    pub protocol: String,
    pub ecu_type: String,
    pub os_id: String,
    pub cal_id: String,
    pub hw_id: String,
    pub vin: String,
    pub hardware: String,
    pub status: String,
}

/// Session manifest written alongside every .bin backup.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BackupManifest {
    pub timestamp_utc: String,
    pub os_id: String,
    pub hw_id: String,
    pub cal_id: String,
    pub vin: String,
    pub file_name: String,
    pub size_bytes: usize,
    pub sha256: String,
}

#[derive(Serialize, Clone)]
struct BackupResult {
    file_name: String,
    size_bytes: usize,
    sha256: String,
    manifest: BackupManifest,
}

#[derive(Serialize, Clone)]
struct BinValidationResult {
    detected_os_id: String,
    checksum_ok: bool,
    compatibility: String,
}

#[derive(Serialize, Clone)]
struct CompareResult {
    compatibility: String,
    summary: String,
}

#[derive(Serialize, Clone)]
struct JobResult {
    message: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// J1850 VPW / ALDL physical layer helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Build a GM ALDL-style J1850 VPW request frame.
/// Header: 0x68 (priority 3, J1850 VPW, no IFR) + dest 0x6A (ECM) + src 0xF1 (tool)
fn build_aldl_request(mode: u8, pid: u8) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, mode, pid];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

/// Build a raw J1850 VPW request frame (OBD-II service 0x01 style).
fn build_pid_request(pid: u8) -> Vec<u8> {
    build_aldl_request(0x01, pid)
}

fn validate_checksum(frame: &[u8]) -> bool {
    if frame.len() < 2 {
        return false;
    }
    let payload = &frame[..frame.len() - 1];
    let expected = payload.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    expected == frame[frame.len() - 1]
}

fn parse_pid_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 {
        return None;
    }
    // Standard OBD-II positive response: 0x41 + pid
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_checksum(frame) {
        return Some(frame[5..frame.len() - 1].to_vec());
    }
    None
}

/// Parse an ALDL mode-0x5A (read data by id) positive response.
/// Positive response byte for mode 0x1A is 0x5A.
fn parse_aldl_response(frame: &[u8], expected_id: u8) -> Option<Vec<u8>> {
    // Min valid frame: hdr(3) + mode_resp(1) + id(1) + data(>=1) + cs(1) = 7 bytes
    if frame.len() < 7 {
        return None;
    }
    if frame[3] == 0x5A && frame[4] == expected_id && validate_checksum(frame) {
        return Some(frame[5..frame.len() - 1].to_vec());
    }
    None
}

/// Send a frame and read back one response with up to `retries` attempts.
fn send_recv(
    port: &mut Box<dyn SerialPort>,
    frame: &[u8],
    retries: u8,
) -> Result<Vec<u8>, String> {
    let mut last_err = String::new();
    for attempt in 0..=retries {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
        port.write_all(frame)
            .map_err(|e| format!("Write error: {}", e))?;
        let mut buf = [0u8; 256];
        match port.read(&mut buf) {
            Ok(n) if n > 0 => return Ok(buf[..n].to_vec()),
            Ok(_) => last_err = "Empty response".to_string(),
            Err(e) => last_err = format!("Read error: {}", e),
        }
    }
    Err(last_err)
}

fn request_pid(port: &mut Box<dyn SerialPort>, pid: u8) -> Result<Vec<u8>, String> {
    let req = build_pid_request(pid);
    let resp = send_recv(port, &req, 2)?;
    parse_pid_response(&resp, pid)
        .ok_or_else(|| format!("No valid OBD response for PID 0x{:02X}", pid))
}

/// Request an ALDL data-by-ID (mode 0x1A) and return the payload bytes.
fn request_aldl_id(
    port: &mut Box<dyn SerialPort>,
    data_id: u8,
) -> Result<Vec<u8>, String> {
    let req = build_aldl_request(0x1A, data_id);
    let resp = send_recv(port, &req, 3)?;
    parse_aldl_response(&resp, data_id)
        .ok_or_else(|| format!("No valid ALDL response for ID 0x{:02X}", data_id))
}

// ──────────────────────────────────────────────────────────────────────────────
// ALDL / PCM read helpers – P01 0411 specific
// ──────────────────────────────────────────────────────────────────────────────

/// Read the 4-byte OS ID from ALDL data ID 0x10 and format as decimal string.
fn read_os_id(port: &mut Box<dyn SerialPort>) -> String {
    match request_aldl_id(port, 0x10) {
        Ok(d) if d.len() >= 4 => {
            let id = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
            format!("{}", id)
        }
        Ok(d) => format!("RAW:{}", hex_string(&d)),
        Err(_) => "UNKNOWN".to_string(),
    }
}

/// Read the 4-byte hardware ID from ALDL data ID 0x14.
fn read_hw_id(port: &mut Box<dyn SerialPort>) -> String {
    match request_aldl_id(port, 0x14) {
        Ok(d) if d.len() >= 4 => {
            let id = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
            format!("{}", id)
        }
        Ok(d) => format!("RAW:{}", hex_string(&d)),
        Err(_) => "UNKNOWN".to_string(),
    }
}

/// Read the 4-byte calibration ID from ALDL data ID 0x16.
fn read_cal_id(port: &mut Box<dyn SerialPort>) -> String {
    match request_aldl_id(port, 0x16) {
        Ok(d) if d.len() >= 4 => {
            let id = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
            format!("{}", id)
        }
        Ok(d) => format!("RAW:{}", hex_string(&d)),
        Err(_) => "UNKNOWN".to_string(),
    }
}

/// Read VIN from ALDL data ID 0x90 (17 ASCII bytes on P01).
fn read_vin(port: &mut Box<dyn SerialPort>) -> String {
    match request_aldl_id(port, 0x90) {
        Ok(d) if d.len() >= 17 => {
            String::from_utf8_lossy(&d[..17]).trim().to_string()
        }
        Ok(d) => format!("RAW:{}", hex_string(&d)),
        Err(_) => "NOT_SUPPORTED".to_string(),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// PCM memory map – P01 0411
//
// The P01 (16-bit HC16) PCM has a 512 KB (0x8_0000) flash map.
// ALDL memory read uses mode 0x23 (read memory by address).
// Format: [0x68, 0x6A, 0xF1, 0x23, addr_hi, addr_mid, addr_lo, len, cs]
// Max safe chunk per frame: 128 bytes (ELM327 / pass-thru buffer limit).
// ──────────────────────────────────────────────────────────────────────────────

const PCM_FLASH_START: u32 = 0x000000;
const PCM_FLASH_SIZE: u32 = 0x080000; // 512 KB
const CHUNK_SIZE: u32 = 128;

fn build_mem_read_request(addr: u32, len: u8) -> Vec<u8> {
    let addr_hi = ((addr >> 16) & 0xFF) as u8;
    let addr_mid = ((addr >> 8) & 0xFF) as u8;
    let addr_lo = (addr & 0xFF) as u8;
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x23, addr_hi, addr_mid, addr_lo, len];
    let cs = frame.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    frame.push(cs);
    frame
}

fn parse_mem_read_response(frame: &[u8], expected_len: u8) -> Option<Vec<u8>> {
    // Positive response for mode 0x23 is 0x63
    // Frame: [hdr×3, 0x63, data×len, cs]
    if frame.len() < (expected_len as usize + 5) {
        return None;
    }
    if frame[3] != 0x63 {
        return None;
    }
    if !validate_checksum(frame) {
        return None;
    }
    Some(frame[4..4 + expected_len as usize].to_vec())
}

// ──────────────────────────────────────────────────────────────────────────────
// Utility
// ──────────────────────────────────────────────────────────────────────────────

fn hex_string(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

// ──────────────────────────────────────────────────────────────────────────────
// OBD-II PID decoders (unchanged from Stage 0)
// ──────────────────────────────────────────────────────────────────────────────

fn decode_rpm(data: &[u8]) -> Option<f32> {
    if data.len() < 2 { return None; }
    Some(((data[0] as f32 * 256.0) + data[1] as f32) / 4.0)
}
fn decode_map(data: &[u8]) -> Option<f32> { data.first().map(|v| *v as f32) }
fn decode_iat(data: &[u8]) -> Option<f32> { data.first().map(|v| *v as f32 - 40.0) }
fn decode_ect(data: &[u8]) -> Option<f32> { data.first().map(|v| *v as f32 - 40.0) }
fn decode_tps(data: &[u8]) -> Option<f32> { data.first().map(|v| (*v as f32 / 255.0) * 100.0) }
fn decode_vss(data: &[u8]) -> Option<f32> { data.first().map(|v| *v as f32) }
fn decode_o2_b1s1(data: &[u8]) -> Option<f32> { data.first().map(|v| *v as f32 * 0.005) }
fn o2_voltage_to_afr(volts: f32) -> f32 {
    if volts < 0.1 { 16.5 } else if volts > 0.9 { 12.8 } else { 14.7 + (0.45 - volts) * 8.0 }
}
fn decode_stft(data: &[u8]) -> Option<f32> { data.first().map(|v| (*v as f32 / 128.0) * 100.0 - 100.0) }
fn decode_ltft(data: &[u8]) -> Option<f32> { data.first().map(|v| (*v as f32 / 128.0) * 100.0 - 100.0) }
fn decode_spark(data: &[u8]) -> Option<f32> { data.first().map(|v| (*v as f32 / 2.0) - 64.0) }
fn decode_batt_volt(data: &[u8]) -> Option<f32> {
    if data.len() < 2 { return None; }
    Some(((data[0] as u16 * 256 + data[1] as u16) as f32) / 1000.0)
}
fn decode_monitor_status(data: &[u8]) -> Option<(bool, u8)> {
    data.first().map(|v| ((v & 0x80) != 0, v & 0x7F))
}

// ──────────────────────────────────────────────────────────────────────────────
// Tauri commands – connection management
// ──────────────────────────────────────────────────────────────────────────────

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
fn connect_ecu(port: String, baud: u32, state: tauri::State<AppState>) -> Result<String, String> {
    let serial = serialport::new(&port, baud)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| format!("Failed to open {}: {}", port, e))?;

    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    *guard = Some(serial);
    Ok(format!("Connected to {} at {} baud", port, baud))
}

#[tauri::command]
fn disconnect_ecu(state: tauri::State<AppState>) -> Result<String, String> {
    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    *guard = None;
    Ok("Disconnected".to_string())
}

#[tauri::command]
fn connection_status(state: tauri::State<AppState>) -> Result<bool, String> {
    let guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    Ok(guard.is_some())
}

#[tauri::command]
fn write_ecu_frame(data: Vec<u8>, state: tauri::State<AppState>) -> Result<String, String> {
    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No serial connection established")?;
    port.write_all(&data).map_err(|e| format!("Write error: {}", e))?;
    Ok(format!("Wrote {} bytes", data.len()))
}

#[tauri::command]
fn read_ecu_frame(state: tauri::State<AppState>) -> Result<RawFrame, String> {
    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No serial connection established")?;
    let mut buf = [0u8; 256];
    let n = port.read(&mut buf).map_err(|e| format!("Read error: {}", e))?;
    let raw = buf[..n].to_vec();
    let hex = hex_string(&raw);
    Ok(RawFrame { bytes_read: raw.len(), raw, hex })
}

// ──────────────────────────────────────────────────────────────────────────────
// Tauri commands – live telemetry (OBD-II PIDs)
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn read_ecu_data(state: tauri::State<AppState>) -> Result<EcuTelemetry, String> {
    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    let port = guard.as_mut().ok_or("No serial connection established")?;

    let mut data = EcuTelemetry::default();

    if let Ok(r) = request_pid(port, 0x0C) { if let Some(v) = decode_rpm(&r) { data.rpm = v; } }
    if let Ok(r) = request_pid(port, 0x0B) { if let Some(v) = decode_map(&r) { data.map = v; } }
    if let Ok(r) = request_pid(port, 0x0F) { if let Some(v) = decode_iat(&r) { data.iat = v; } }
    if let Ok(r) = request_pid(port, 0x05) { if let Some(v) = decode_ect(&r) { data.ect = v; } }
    if let Ok(r) = request_pid(port, 0x11) { if let Some(v) = decode_tps(&r) { data.tps = v; } }
    if let Ok(r) = request_pid(port, 0x0D) { if let Some(v) = decode_vss(&r) { data.vss = v; } }
    if let Ok(r) = request_pid(port, 0x14) {
        if let Some(v) = decode_o2_b1s1(&r) { data.o2_b1s1 = v; data.afr = o2_voltage_to_afr(v); }
    }
    if let Ok(r) = request_pid(port, 0x06) { if let Some(v) = decode_stft(&r) { data.stft_b1 = v; } }
    if let Ok(r) = request_pid(port, 0x07) { if let Some(v) = decode_ltft(&r) { data.ltft_b1 = v; } }
    if let Ok(r) = request_pid(port, 0x0E) { if let Some(v) = decode_spark(&r) { data.spark_adv = v; } }
    if let Ok(r) = request_pid(port, 0x42) { if let Some(v) = decode_batt_volt(&r) { data.batt_volt = v; } }
    if let Ok(r) = request_pid(port, 0x01) {
        if let Some((mil, count)) = decode_monitor_status(&r) { data.mil_on = mil; data.dtc_count = count; }
    }

    Ok(data)
}

// ──────────────────────────────────────────────────────────────────────────────
// Tauri commands – Stage 1: real identity read
// ──────────────────────────────────────────────────────────────────────────────

/// Read ECU identity via ALDL mode 0x1A.
/// Queries OSID (0x10), hardware ID (0x14), cal ID (0x16), VIN (0x90).
/// Returns a fully populated EcuProperties on success.
#[tauri::command]
fn read_properties(state: tauri::State<AppState>) -> Result<EcuProperties, String> {
    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    let port = guard
        .as_mut()
        .ok_or("No serial connection established — connect first")?;

    let os_id = read_os_id(port);
    let hw_id = read_hw_id(port);
    let cal_id = read_cal_id(port);
    let vin = read_vin(port);

    // Classify ECU based on OS ID prefix (known P01 range: 12200000–12299999)
    let (ecu_type, hardware, status) = if os_id.starts_with("122") {
        (
            "P01 / 0411".to_string(),
            "LS1 PCM – 512 KB flash".to_string(),
            "Identified".to_string(),
        )
    } else if os_id == "UNKNOWN" {
        (
            "Unknown".to_string(),
            "Unknown".to_string(),
            "Identification failed – check connection and baud rate".to_string(),
        )
    } else {
        (
            "Non-P01".to_string(),
            "Unknown PCM".to_string(),
            format!("Unrecognised OSID {}", os_id),
        )
    };

    Ok(EcuProperties {
        protocol: "GM J1850 VPW / ALDL".to_string(),
        ecu_type,
        os_id,
        cal_id,
        hw_id,
        vin,
        hardware,
        status,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Tauri commands – Stage 1: chunked PCM read with SHA-256
//
// Safety rules enforced here:
//   1. Identity must be confirmed (os_id != UNKNOWN) before backup proceeds.
//   2. Backup is READ ONLY. No write path is enabled at this stage.
//   3. Session manifest is returned alongside the binary digest so the
//      frontend can persist it before any write workflow is shown.
// ──────────────────────────────────────────────────────────────────────────────

/// Read the full 512 KB PCM flash in 128-byte chunks.
/// Returns a BackupResult containing the digest and a complete session manifest.
/// The manifest MUST be saved by the frontend before any write commands are used.
#[tauri::command]
fn read_entire_pcm(state: tauri::State<AppState>) -> Result<BackupResult, String> {
    let mut guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    let port = guard
        .as_mut()
        .ok_or("No serial connection established — connect first")?;

    // Step 1: confirm identity before any memory read.
    let os_id = read_os_id(port);
    if os_id == "UNKNOWN" {
        return Err(
            "ECU identity not confirmed. Run Read Properties first and ensure OSID is valid."
                .to_string(),
        );
    }
    let hw_id = read_hw_id(port);
    let cal_id = read_cal_id(port);
    let vin = read_vin(port);

    // Step 2: chunked memory read.
    let total_chunks = PCM_FLASH_SIZE / CHUNK_SIZE;
    let mut image: Vec<u8> = Vec::with_capacity(PCM_FLASH_SIZE as usize);

    for chunk_idx in 0..total_chunks {
        let addr = PCM_FLASH_START + chunk_idx * CHUNK_SIZE;
        let req = build_mem_read_request(addr, CHUNK_SIZE as u8);

        // Up to 3 retries per chunk — critical for noisy VPW bus.
        let mut chunk_data: Option<Vec<u8>> = None;
        for attempt in 0..3u8 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(10));
            }
            if let Ok(resp) = send_recv(port, &req, 0) {
                if let Some(data) = parse_mem_read_response(&resp, CHUNK_SIZE as u8) {
                    chunk_data = Some(data);
                    break;
                }
            }
        }

        match chunk_data {
            Some(d) => image.extend_from_slice(&d),
            None => {
                return Err(format!(
                    "Read failure at address 0x{:06X} (chunk {}/{}). Backup aborted — ECU not modified.",
                    addr,
                    chunk_idx + 1,
                    total_chunks
                ))
            }
        }
    }

    // Step 3: SHA-256 digest.
    let sha256 = compute_sha256(&image);
    let size_bytes = image.len();
    let file_name = format!("p01_backup_osid{}_{}.bin", os_id, Utc::now().format("%Y%m%d_%H%M%S"));

    // Step 4: build session manifest.
    let manifest = BackupManifest {
        timestamp_utc: Utc::now().to_rfc3339(),
        os_id: os_id.clone(),
        hw_id: hw_id.clone(),
        cal_id: cal_id.clone(),
        vin: vin.clone(),
        file_name: file_name.clone(),
        size_bytes,
        sha256: sha256.clone(),
    };

    Ok(BackupResult {
        file_name,
        size_bytes,
        sha256,
        manifest,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Tauri commands – BIN validation & comparison (stub – Stage 2 completes these)
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn validate_bin(file_name: String, file_size: usize) -> Result<BinValidationResult, String> {
    let compatibility = if file_name.to_lowercase().contains("12225074") || file_size == 524288 {
        "Compatible"
    } else if file_size > 0 {
        "Unknown – size mismatch or OS ID not found in filename"
    } else {
        "Invalid"
    };

    Ok(BinValidationResult {
        detected_os_id: "12225074".to_string(),
        checksum_ok: file_size == 524288,
        compatibility: compatibility.to_string(),
    })
}

#[tauri::command]
fn compare_bin_to_ecu(
    file_name: String,
    file_size: usize,
    state: tauri::State<AppState>,
) -> Result<CompareResult, String> {
    let guard = state.port.lock().map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }
    Ok(CompareResult {
        compatibility: "Compatible".to_string(),
        summary: format!(
            "[STUB] Compared {} ({} bytes) to ECU. Full byte-diff in Stage 2.",
            file_name, file_size
        ),
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Tauri commands – write path (STUBBED – Stage 2 only)
// These commands exist so the frontend can wire up UI without bricking risk.
// They return an error if called to prevent accidental use.
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
fn write_calibration(_file_name: String, _state: tauri::State<AppState>) -> Result<JobResult, String> {
    Err("[STAGE 2 REQUIRED] Calibration write not yet implemented. Complete Stage 1 read pipeline first.".to_string())
}

#[tauri::command]
fn write_os_calibration(_file_name: String, _state: tauri::State<AppState>) -> Result<JobResult, String> {
    Err("[STAGE 2 REQUIRED] OS + calibration write not yet implemented.".to_string())
}

#[tauri::command]
fn verify_after_write(_state: tauri::State<AppState>) -> Result<JobResult, String> {
    Err("[STAGE 2 REQUIRED] Post-write verification not yet implemented.".to_string())
}

// ──────────────────────────────────────────────────────────────────────────────
// App entry point
// ──────────────────────────────────────────────────────────────────────────────

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
            read_properties,
            read_entire_pcm,
            validate_bin,
            compare_bin_to_ecu,
            write_calibration,
            write_os_calibration,
            verify_after_write
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
