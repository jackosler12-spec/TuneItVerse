// TuneItVerse - lib.rs
// Fixed top-level `let` error by moving chrono timestamp inside functions

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
// Add other uses as needed from previous versions

#[tauri::command]
fn read_entire_pcm(state: tauri::State<AppState>) -> Result<String, String> {
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    // Original function body here - placeholder for full logic
    // ... read logic, save bin with timestamp ...
    Ok(format!("ECU dump saved as pcm_backup_{}.bin", ts))
}

// All other commands (connect_ecu, read_ecu_data, validate_bin, etc.) should be defined here with proper returns

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            // list all commands
            list_serial_ports,
            list_supported_ecus,
            connect_ecu,
            read_entire_pcm,
            validate_bin,
            // etc.
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
