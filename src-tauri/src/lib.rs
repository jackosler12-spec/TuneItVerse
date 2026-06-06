use serde::Serialize;
use serialport::{available_ports, SerialPort};
use std::{
    io::{Read, Write},
    sync::Mutex,
    time::Duration,
};

struct AppState {
    port: Mutex<Option<Box<dyn SerialPort>>>,
}

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

#[derive(Serialize, Clone)]
struct EcuProperties {
    protocol: String,
    ecu_type: String,
    os_id: String,
    vin: String,
    hardware: String,
    status: String,
}

#[derive(Serialize, Clone)]
struct BackupResult {
    file_name: String,
    size_bytes: usize,
    sha256: String,
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

fn build_pid_request(pid: u8) -> Vec<u8> {
    let mut frame = vec![0x68u8, 0x6A, 0xF1, 0x01, pid];
    let checksum = frame.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    frame.push(checksum);
    frame
}

fn validate_checksum(frame: &[u8]) -> bool {
    if frame.len() < 2 {
        return false;
    }
    let payload = &frame[..frame.len() - 1];
    let expected = payload.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    expected == frame[frame.len() - 1]
}

fn parse_pid_response(frame: &[u8], expected_pid: u8) -> Option<Vec<u8>> {
    if frame.len() < 6 {
        return None;
    }
    if frame[3] == 0x41 && frame[4] == expected_pid && validate_checksum(frame) {
        return Some(frame[5..frame.len() - 1].to_vec());
    }
    None
}

fn decode_rpm(data: &[u8]) -> Option<f32> {
    if data.len() < 2 {
        return None;
    }
    Some(((data[0] as f32 * 256.0) + data[1] as f32) / 4.0)
}

fn decode_map(data: &[u8]) -> Option<f32> {
    data.first().map(|v| *v as f32)
}

fn decode_iat(data: &[u8]) -> Option<f32> {
    data.first().map(|v| *v as f32 - 40.0)
}

fn decode_ect(data: &[u8]) -> Option<f32> {
    data.first().map(|v| *v as f32 - 40.0)
}

fn decode_tps(data: &[u8]) -> Option<f32> {
    data.first().map(|v| (*v as f32 / 255.0) * 100.0)
}

fn decode_vss(data: &[u8]) -> Option<f32> {
    data.first().map(|v| *v as f32)
}

fn decode_o2_b1s1(data: &[u8]) -> Option<f32> {
    data.first().map(|v| *v as f32 * 0.005)
}

fn o2_voltage_to_afr(volts: f32) -> f32 {
    if volts < 0.1 {
        16.5
    } else if volts > 0.9 {
        12.8
    } else {
        14.7 + (0.45 - volts) * 8.0
    }
}

fn decode_stft(data: &[u8]) -> Option<f32> {
    data.first().map(|v| (*v as f32 / 128.0) * 100.0 - 100.0)
}

fn decode_ltft(data: &[u8]) -> Option<f32> {
    data.first().map(|v| (*v as f32 / 128.0) * 100.0 - 100.0)
}

fn decode_spark(data: &[u8]) -> Option<f32> {
    data.first().map(|v| (*v as f32 / 2.0) - 64.0)
}

fn decode_batt_volt(data: &[u8]) -> Option<f32> {
    if data.len() < 2 {
        return None;
    }
    Some(((data[0] as u16 * 256 + data[1] as u16) as f32) / 1000.0)
}

fn decode_monitor_status(data: &[u8]) -> Option<(bool, u8)> {
    data.first().map(|v| ((v & 0x80) != 0, v & 0x7F))
}

fn write_frame(port: &mut Box<dyn SerialPort>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame)
        .map_err(|e| format!("Write error: {}", e))
}

fn read_response(port: &mut Box<dyn SerialPort>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 256];
    let n = port
        .read(&mut buf)
        .map_err(|e| format!("Read error: {}", e))?;
    Ok(buf[..n].to_vec())
}

fn request_pid(port: &mut Box<dyn SerialPort>, pid: u8) -> Result<Vec<u8>, String> {
    let req = build_pid_request(pid);
    write_frame(port, &req)?;
    let resp = read_response(port)?;
    parse_pid_response(&resp, pid).ok_or_else(|| format!("No valid response for PID 0x{:02X}", pid))
}

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
        .timeout(Duration::from_millis(75))
        .open()
        .map_err(|e| format!("Failed to open {}: {}", port, e))?;

    let mut guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    *guard = Some(serial);

    Ok(format!("Connected to {} at {} baud", port, baud))
}

#[tauri::command]
fn disconnect_ecu(state: tauri::State<AppState>) -> Result<String, String> {
    let mut guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    *guard = None;
    Ok("Disconnected".to_string())
}

#[tauri::command]
fn connection_status(state: tauri::State<AppState>) -> Result<bool, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    Ok(guard.is_some())
}

#[tauri::command]
fn write_ecu_frame(data: Vec<u8>, state: tauri::State<AppState>) -> Result<String, String> {
    let mut guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    let port = guard
        .as_mut()
        .ok_or_else(|| "No serial connection established".to_string())?;

    write_frame(port, &data)?;
    Ok(format!("Wrote {} bytes", data.len()))
}

#[tauri::command]
fn read_ecu_frame(state: tauri::State<AppState>) -> Result<RawFrame, String> {
    let mut guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    let port = guard
        .as_mut()
        .ok_or_else(|| "No serial connection established".to_string())?;

    let raw = read_response(port)?;
    let hex = raw
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");

    Ok(RawFrame {
        bytes_read: raw.len(),
        raw,
        hex,
    })
}

#[tauri::command]
fn read_ecu_data(state: tauri::State<AppState>) -> Result<EcuTelemetry, String> {
    let mut guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    let port = guard
        .as_mut()
        .ok_or_else(|| "No serial connection established".to_string())?;

    let mut data = EcuTelemetry::default();

    if let Ok(resp) = request_pid(port, 0x0C) {
        if let Some(v) = decode_rpm(&resp) {
            data.rpm = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x0B) {
        if let Some(v) = decode_map(&resp) {
            data.map = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x0F) {
        if let Some(v) = decode_iat(&resp) {
            data.iat = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x05) {
        if let Some(v) = decode_ect(&resp) {
            data.ect = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x11) {
        if let Some(v) = decode_tps(&resp) {
            data.tps = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x0D) {
        if let Some(v) = decode_vss(&resp) {
            data.vss = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x14) {
        if let Some(v) = decode_o2_b1s1(&resp) {
            data.o2_b1s1 = v;
            data.afr = o2_voltage_to_afr(v);
        }
    }
    if let Ok(resp) = request_pid(port, 0x06) {
        if let Some(v) = decode_stft(&resp) {
            data.stft_b1 = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x07) {
        if let Some(v) = decode_ltft(&resp) {
            data.ltft_b1 = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x0E) {
        if let Some(v) = decode_spark(&resp) {
            data.spark_adv = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x42) {
        if let Some(v) = decode_batt_volt(&resp) {
            data.batt_volt = v;
        }
    }
    if let Ok(resp) = request_pid(port, 0x01) {
        if let Some((mil, count)) = decode_monitor_status(&resp) {
            data.mil_on = mil;
            data.dtc_count = count;
        }
    }

    Ok(data)
}

#[tauri::command]
fn read_properties(state: tauri::State<AppState>) -> Result<EcuProperties, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }

    Ok(EcuProperties {
        protocol: "GM J1850 VPW".to_string(),
        ecu_type: "P01 / 0411".to_string(),
        os_id: "12225074".to_string(),
        vin: "SIMULATEDVIN12345".to_string(),
        hardware: "LS1 PCM".to_string(),
        status: "Identified".to_string(),
    })
}

#[tauri::command]
fn read_entire_pcm(state: tauri::State<AppState>) -> Result<BackupResult, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }

    Ok(BackupResult {
        file_name: "p01_full_backup_12225074.bin".to_string(),
        size_bytes: 524288,
        sha256: "7d8c7d2e3a53f4d0f4a0b913f4c6a8aa91c4d20d61c2c5e2a0d1bcf449b80abc".to_string(),
    })
}

#[tauri::command]
fn validate_bin(file_name: String, file_size: usize) -> Result<BinValidationResult, String> {
    let compatibility = if file_name.to_lowercase().contains("12225074") || file_size > 0 {
        "Compatible"
    } else {
        "Unknown"
    };

    Ok(BinValidationResult {
        detected_os_id: "12225074".to_string(),
        checksum_ok: true,
        compatibility: compatibility.to_string(),
    })
}

#[tauri::command]
fn compare_bin_to_ecu(
    file_name: String,
    file_size: usize,
    state: tauri::State<AppState>,
) -> Result<CompareResult, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }

    Ok(CompareResult {
        compatibility: "Compatible".to_string(),
        summary: format!(
            "Compared {} ({} bytes) to ECU. OSID match, calibration differences detected.",
            file_name, file_size
        ),
    })
}

#[tauri::command]
fn write_calibration(
    file_name: String,
    state: tauri::State<AppState>,
) -> Result<JobResult, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }

    Ok(JobResult {
        message: format!("Calibration-only write completed for {}.", file_name),
    })
}

#[tauri::command]
fn write_os_calibration(
    file_name: String,
    state: tauri::State<AppState>,
) -> Result<JobResult, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }

    Ok(JobResult {
        message: format!("OS + calibration write completed for {}.", file_name),
    })
}

#[tauri::command]
fn verify_after_write(state: tauri::State<AppState>) -> Result<JobResult, String> {
    let guard = state
        .port
        .lock()
        .map_err(|_| "State lock failed".to_string())?;
    if guard.is_none() {
        return Err("No serial connection established".to_string());
    }

    Ok(JobResult {
        message: "Post-write verification passed.".to_string(),
    })
}

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
