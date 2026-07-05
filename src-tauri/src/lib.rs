// Standalone command for manual session control

#[tauri::command]
async fn enter_diagnostic_session_cmd(
    state: State<'_, AppState>,
    session_type: u8,
) -> Result<String, String> {
    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        enter_diagnostic_session(device, session_type)
            .map_err(|e| format!("Failed to enter session 0x{:02X}: {}", session_type, e))?;

        return Ok(format!("Successfully entered diagnostic session 0x{:02X}", session_type));
    }

    Err("No J2534 device connected".into())
}