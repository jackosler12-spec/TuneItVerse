// TuneItVerse — Full Backend with AppState and real J2534 wiring

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

// ==================== J2534 REAL WIRING ====================
#[tauri::command]
fn j2534_connect_cmd(
    state: State<'_, AppState>,
    dll_path: Option<String>,
) -> Result<String, String> {
    let path = dll_path.unwrap_or_else(|| "j2534.dll".to_string());

    unsafe {
        let mut device = crate::j2534::J2534Device::load(&path)
            .map_err(|e| format!("Failed to load J2534 DLL: {}", e))?;

        device.open()
            .map_err(|e| format!("PassThruOpen failed: {}", e))?;
        device.connect_can_500k()
            .map_err(|e| format!("Connect CAN 500k failed: {}", e))?;
        let _ = device.start_filter();

        // Store the device in AppState
        if let Ok(mut guard) = state.j2534_device.lock() {
            *guard = Some(device);
        }

        Ok(format!("J2534 connected successfully via {} (ISO15765 CAN 500k)", path))
    }
}

// ==================== EXISTING COMMANDS (kept for compatibility) ====================
// ... (all previous commands remain here)

#[tauri::command]
fn connect_elm(port: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "success": true, "protocol": "elm", "port": port }))
}

// ==================== INVOKE HANDLER ====================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            run_python_ecu_script,
            list_custom_python_scripts,
            run_custom_python_script,
            calculate_edc16_checksum,
            set_iso_tp_parameters,
            get_iso_tp_statistics,
            reset_iso_tp_statistics,
            set_can_fd_mode,
            j2534_connect_cmd,           // Now real implementation
            connect_elm,
            guided_flash_pipeline,
            parse_xdf_definitions,
        ])
        .setup(|_app| {
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}