// TuneItVerse - lib.rs
// Updated for Pillar 1 completion: added guided_flash_pipeline and get_recovery_prompt commands

use tauri::Manager;
mod checksum;
mod dtc;
mod ecu_database;
mod flash;
mod pid_decode;
mod security;
mod vpw;
mod xdf;

use crate::checksum::ChecksumReport;
use crate::flash::{GuidedFlashRequest, GuidedFlashResult, RecoveryPrompt}; // New Pillar 1 types

// Existing placeholder command
#[tauri::command]
fn read_entire_pcm(state: tauri::State<AppState>) -> Result<String, String> {
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    // ... full logic ...
    Ok(format!("ECU dump saved as pcm_backup_{}.bin", ts))
}

// New Pillar 1 orchestration commands (implemented in flash.rs)
// These complete the guided safe flashing pipeline

#[tauri::command]
fn guided_flash_pipeline(request_json: String) -> Result<String, String> {
    flash::guided_flash_pipeline(request_json)
}

#[tauri::command]
fn get_recovery_prompt(ecu_family: String, error_context: String) -> Result<String, String> {
    flash::get_recovery_prompt(ecu_family, error_context)
}

// Other commands (connect_ecu, validate_bin, list_supported_ecus, etc.) remain

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_serial_ports,
            list_supported_ecus,
            connect_ecu,
            read_entire_pcm,
            validate_bin,
            // Pillar 1 new commands
            guided_flash_pipeline,
            get_recovery_prompt,
            // etc.
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
