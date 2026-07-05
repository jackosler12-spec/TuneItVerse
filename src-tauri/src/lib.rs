// Security Access Tauri Command

#[tauri::command]
async fn perform_security_access_cmd(
    state: State<'_, AppState>,
    level: u8,
    family: String,
) -> Result<String, String> {
    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        unsafe {
            crate::security::perform_security_access(device, level, &family)
                .map_err(|e| format!("Security Access failed: {}", e))?;
        }
        return Ok(format!("Security Access Level {} successful for {}", level, family));
    }

    Err("No J2534 device connected for Security Access".into())
}

// Optional: Auto security unlock before live patch (can be called from frontend)
// For now we keep apply_live_patch clean but this command is available