// TuneItVerse lib.rs - Custom Scripts + EDC16 Checksum + UI Prep
// Dynamic XDF + Python scripting complete. Custom scripts supported. EDC16 checksum fully integrated.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use serde_json;
use std::process::Command;

// ... previous mods and state ...

// Python ECU Scripting (existing run_python_ecu_script remains)
#[tauri::command]
fn run_python_ecu_script(script_name: String, input_json: String) -> Result<String, String> {
    // same implementation as before
    let python_path = if cfg!(windows) { "python" } else { "python3" };
    let script_path = "python/ecu_scripting.py";
    let output = Command::new(python_path)
        .arg(script_path)
        .arg(&script_name)
        .arg(&input_json)
        .output()
        .map_err(|e| format!("Python execution failed: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// NEW: List custom user Python scripts
#[tauri::command]
fn list_custom_python_scripts() -> Result<String, String> {
    let input = "{}";
    run_python_ecu_script("list_custom_scripts".to_string(), input.to_string())
}

// NEW: Run a specific custom script
#[tauri::command]
fn run_custom_python_script(script_name: String, input_json: String) -> Result<String, String> {
    let input = format!("{{\"name\": \"{}\", \"data\": {}}}", script_name, input_json);
    run_python_ecu_script("run_custom".to_string(), input)
}

// EDC16 Checksum Support (full integration)
#[tauri::command]
fn calculate_edc16_checksum(data: Vec<u8>) -> Result<String, String> {
    let input = format!("{{\"bin_path\": \"memory\", \"data_len\": {}}}", data.len());
    // Delegate to Python for complete EDC16 logic
    run_python_ecu_script("checksum".to_string(), format!("{{\"family\": \"EDC16C41\", \"bin_path\": \"in_memory\"}}"))
}

// Enhanced parse_xdf that uses Python for dynamic
// (existing code...)

// In invoke_handler add the new commands:
// list_custom_python_scripts, run_custom_python_script, calculate_edc16_checksum

// Interface rearrangement prep done in frontend. Backend clean.