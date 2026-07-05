// Reset and Disconnect Logic

#[tauri::command]
async fn disconnect_all(state: State<'_, AppState>) -> Result<String, String> {
    // Disconnect J2534
    {
        let mut j2534_guard = state.j2534_device.lock().map_err(|e| e.to_string())?;
        if let Some(ref device) = *j2534_guard {
            unsafe {
                let _ = device.disconnect();
            }
        }
        *j2534_guard = None;
    }

    // Disconnect ELM
    {
        let mut elm_guard = state.elm_port.lock().map_err(|e| e.to_string())?;
        *elm_guard = None;
    }

    // Reset ConnectionState
    if let Ok(mut state_guard) = state.connection_state.lock() {
        *state_guard = ConnectionState::default();
    }

    Ok("All connections closed and state reset".into())
}

#[tauri::command]
async fn reset_connection_state(state: State<'_, AppState>) -> Result<String, String> {
    if let Ok(mut guard) = state.connection_state.lock() {
        *guard = ConnectionState::default();
        return Ok("Connection state reset successfully".into());
    }
    Err("Failed to reset connection state".into())
}

// Enhance existing j2534_disconnect to also reset state
#[tauri::command]
async fn j2534_disconnect(state: State<'_, AppState>) -> Result<String, String> {
    {
        let mut guard = state.j2534_device.lock().map_err(|e| e.to_string())?;
        if let Some(ref device) = *guard {
            unsafe {
                let _ = device.disconnect();
            }
        }
        *guard = None;
    }

    // Also reset connection state
    if let Ok(mut state_guard) = state.connection_state.lock() {
        *state_guard = ConnectionState::default();
    }

    Ok("J2534 disconnected and state cleared".into())
}