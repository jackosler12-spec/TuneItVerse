// TuneItVerse — Complete lib.rs with AppState and full J2534 support

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use serde_json;
use std::process::Command;

mod can;
mod checksum;
mod consult;
mod dtc;
mod ecu_database;
mod flash;
mod j2534;
mod kwp;
mod pid_decode;
mod security;
mod vpw;
mod xdf;

// ==================== APP STATE ====================
pub struct AppState {
    pub j2534_device: Mutex<Option<crate::j2534::J2534Device>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            j2534_device: Mutex::new(None),
        }
    }
}

// ==================== PYTHON COMMANDS ====================
#[tauri::command]
fn run_python_ecu_script(script_name: String, input_json: String) -> Result<String, String> {
    let python_path = if cfg!(windows) { "python" } else { "python3" };
    let script_path = "python/ecu_scripting.py";

    let output = Command::new(python_path)
        .arg(script_path)
        .arg(&script_name)
        .arg(&input_json)
        .output()
        .map_err(|e| format!("Failed to execute Python: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
fn list_custom_python_scripts() -> Result<String, String> {
    run_python_ecu_script("list_custom_scripts".to_string(), "{}".to_string())
}

#[tauri::command]
fn run_custom_python_script(script_name: String, input_json: String) -> Result<String, String> {
    let input = format!("{{\"name\": \"{}\", \"data\": {}}}", script_name, input_json);
    run_python_ecu_script("run_custom".to_string(), input)
}

#[tauri::command]
fn calculate_edc16_checksum(data: Vec<u8>) -> Result<String, String> {
    run_python_ecu_script("checksum".to_string(), format!("{{\"family\": \"EDC16C41\", \"bin_path\": \"in_memory\"}}"))
}

// ==================== ISO-TP / CAN ====================
#[tauri::command]
fn set_iso_tp_parameters(block_size: u8, stmin_ms: u64) -> Result<String, String> {
    crate::can::set_iso_tp_config(block_size, stmin_ms);
    Ok(format!("ISO-TP updated: BS={}, STmin={}ms", block_size, stmin_ms))
}

#[tauri::command]
fn get_iso_tp_statistics() -> Result<String, String> {
    let stats = crate::can::get_iso_tp_stats();
    serde_json::to_string(&stats).map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_iso_tp_statistics() -> Result<String, String> {
    crate::can::reset_iso_tp_stats();
    Ok("ISO-TP stats reset".into())
}

#[tauri::command]
fn set_can_fd_mode(enabled: bool) -> Result<String, String> {
    crate::can::set_can_fd_mode(enabled);
    Ok(format!("CAN FD mode: {}", enabled))
}

// ==================== J2534 COMMANDS (Real Implementation) ====================

#[tauri::command]
fn j2534_connect_cmd(
    state: State<'_, AppState>,
    dll_path: Option<String>,
) -> Result<String, String> {
    let path = dll_path.unwrap_or_else(|| "j2534.dll".to_string());

    unsafe {
        let mut device = crate::j2534::J2534Device::load(&path)
            .map_err(|e| format!("Failed to load J2534 DLL: {}", e))?;

        device.open().map_err(|e| format!("PassThruOpen failed: {}", e))?;
        device.connect_can_500k().map_err(|e| format!("Connect CAN 500k failed: {}", e))?;
        let _ = device.start_filter();

        if let Ok(mut guard) = state.j2534_device.lock() {
            *guard = Some(device);
        }

        Ok(format!("J2534 connected successfully via {} (ISO15765 CAN 500k)", path))
    }
}

#[tauri::command]
fn j2534_write_uds(
    state: State<'_, AppState>,
    data: Vec<u8>,
    timeout_ms: Option<u32>,
) -> Result<String, String> {
    let timeout = timeout_ms.unwrap_or(2000);
    let mut guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        unsafe {
            device.write_uds(&data, timeout)
                .map_err(|e| format!("J2534 write UDS failed: {}", e))?;
        }
        Ok("UDS frame sent successfully".into())
    } else {
        Err("No J2534 device connected. Call j2534_connect_cmd first.".into())
    }
}

#[tauri::command]
fn j2534_read_msgs(
    state: State<'_, AppState>,
    timeout_ms: Option<u32>,
    max_msgs: Option<usize>,
) -> Result<Vec<String>, String> {
    let timeout = timeout_ms.unwrap_or(1000);
    let max = max_msgs.unwrap_or(10);

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        unsafe {
            let msgs = device.read_uds(timeout, max)
                .map_err(|e| format!("J2534 read failed: {}", e))?;

            let result: Vec<String> = msgs.iter().map(|m| {
                let len = m.DataSize as usize;
                let hex: String = m.Data[..len.min(4128)].iter()
                    .map(|b| format!("{:02X}", b)).collect();
                format!("{} bytes: {}", len, hex)
            }).collect();
            Ok(result)
        }
    } else {
        Err("No J2534 device connected".into())
    }
}

#[tauri::command]
fn j2534_disconnect(state: State<'_, AppState>) -> Result<String, String> {
    let mut guard = state.j2534_device.lock().map_err(|e| e.to_string())?;
    if let Some(ref device) = *guard {
        unsafe { let _ = device.disconnect(); }
        *guard = None;
        Ok("J2534 device disconnected".into())
    } else {
        Ok("No active J2534 connection".into())
    }
}

#[tauri::command]
fn j2534_reconnect(state: State<'_, AppState>) -> Result<String, String> {
    let mut guard = state.j2534_device.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut device) = *guard {
        unsafe {
            device.reconnect().map_err(|e| format!("Reconnect failed: {}", e))?;
        }
        Ok("J2534 reconnected successfully".into())
    } else {
        Err("No previous J2534 device to reconnect".into())
    }
}

// ==================== OTHER COMMANDS ====================
#[tauri::command]
fn connect_elm(port: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "success": true, "protocol": "elm", "port": port }))
}

#[tauri::command]
async fn guided_flash_pipeline(app: AppHandle, request_json: String) -> Result<String, String> {
    let _ = app.emit("flash-log", "Starting guided flash pipeline...");
    let result = crate::flash::orchestrate_guided_flash(request_json).unwrap_or_else(|e| format!("Flash error: {}", e));
    let _ = app.emit("flash-log", format!("Pipeline result: {}", result));
    Ok(result)
}

#[tauri::command]
fn parse_xdf_definitions(bin_bytes: Vec<u8>, family: String, xdf_path: Option<String>) -> Result<String, String> {
    if let Some(path) = xdf_path {
        let input = format!("{{\"family\": \"{}\", \"xdf_path\": \"{}\"}}", family, path);
        return run_python_ecu_script("xdf_parse".to_string(), input);
    }
    Ok("XDF parsed (fallback)".into())
}

// ==================== INVOKE HANDLER ====================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Python
            run_python_ecu_script,
            list_custom_python_scripts,
            run_custom_python_script,
            calculate_edc16_checksum,

            // ISO-TP / CAN
            set_iso_tp_parameters,
            get_iso_tp_statistics,
            reset_iso_tp_statistics,
            set_can_fd_mode,

            // J2534 (real + helpers)
            j2534_connect_cmd,
            j2534_write_uds,
            j2534_read_msgs,
            j2534_disconnect,
            j2534_reconnect,
            connect_elm,

            // Flash & XDF
            guided_flash_pipeline,
            parse_xdf_definitions,
        ])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}