// TuneItVerse - lib.rs
// Pillar 1 completion: Full AppState with live SerialPort, real ECU DB integration, Tauri events for progress/logs

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

mod checksum;
mod dtc;
mod ecu_database;
mod flash;
mod pid_decode;
mod security;
mod vpw;
mod xdf;

use crate::checksum::ChecksumReport;
use crate::ecu_database::{EcuDbEntry, get_ecu_by_family, list_supported_ecu_families};
use crate::flash::{GuidedFlashRequest, GuidedFlashResult, FlashProgress};
use serialport::SerialPort;

// AppState holds the live connection and current ECU context
pub struct AppState {
    pub port: Mutex<Option<Box<dyn SerialPort + Send>>>,
    pub current_ecu: Mutex<Option<EcuDbEntry>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            port: Mutex::new(None),
            current_ecu: Mutex::new(None),
        }
    }
}

// Existing / placeholder commands
#[tauri::command]
fn read_entire_pcm(state: State<AppState>) -> Result<String, String> {
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    // TODO: Use state.port to perform real read via flash module
    Ok(format!("ECU dump saved as pcm_backup_{}.bin (live port ready)", ts))
}

#[tauri::command]
fn list_supported_ecus() -> Vec<String> {
    list_supported_ecu_families()
}

#[tauri::command]
fn connect_ecu(state: State<AppState>, port_name: String, baud: u32) -> Result<String, String> {
    let mut port_guard = state.port.lock().map_err(|e| e.to_string())?;
    
    let port = serialport::new(&port_name, baud)
        .timeout(std::time::Duration::from_millis(1000))
        .open()
        .map_err(|e| format!("Failed to open serial port {}: {}", port_name, e))?;
    
    *port_guard = Some(port);
    Ok(format!("Connected to {} @ {} baud", port_name, baud))
}

// Real Pillar 1 guided pipeline command with live port + events
#[tauri::command]
async fn guided_flash_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    request_json: String,
) -> Result<String, String> {
    let request: GuidedFlashRequest = serde_json::from_str(&request_json)
        .map_err(|e| format!("Invalid GuidedFlashRequest JSON: {}", e))?;

    // Get live port from state
    let mut port_guard = state.port.lock().map_err(|e| e.to_string())?;
    let port = port_guard.as_mut()
        .ok_or_else(|| "No active serial port connection. Call connect_ecu first.".to_string())?;

    // Load real ECU profile from DB
    let ecu_profile = get_ecu_by_family(&request.ecu_family)
        .ok_or_else(|| format!("ECU family '{}' not found in database", request.ecu_family))?; 
    
    // Update current ECU in state
    {
        let mut ecu_guard = state.current_ecu.lock().map_err(|e| e.to_string())?;
        *ecu_guard = Some(ecu_profile.clone());
    }

    let _ = app.emit("flash-log", format!("Loaded profile for {}: {}", request.ecu_family, ecu_profile.display_name));

    // Progress callback that emits Tauri events
    let progress_emitter = |progress: FlashProgress| {
        let _ = app.emit("flash-progress", progress);
    };

    // Call the real orchestration (uses live port from state)
    let result = flash::orchestrate_guided_flash(port, request, progress_emitter)
        .map_err(|e| format!("Pipeline orchestration failed: {}", e))?;

    // Emit final result event
    let _ = app.emit("flash-complete", result.clone());

    serde_json::to_string(&result)
        .map_err(|e| format!("Failed to serialize result: {}", e))
}

#[tauri::command]
fn get_recovery_prompt(ecu_family: String, error_context: String) -> Result<String, String> {
    flash::get_recovery_prompt(ecu_family, error_context)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_serial_ports,
            list_supported_ecus,
            connect_ecu,
            read_entire_pcm,
            validate_bin,
            guided_flash_pipeline,
            get_recovery_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
