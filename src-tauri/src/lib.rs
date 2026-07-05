// lib.rs — Extended J2534 commands with reconnection and error recovery

// ... (previous code remains)

// ==================== ENHANCED J2534 COMMANDS ====================

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
            Ok("UDS frame sent successfully".into())
        }
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

            // Convert to simple hex strings for frontend
            let result: Vec<String> = msgs.iter().map(|m| {
                let len = m.DataSize as usize;
                let hex: String = m.Data[..len.min(4128)].iter()
                    .map(|b| format!("{:02X}", b))
                    .collect();
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
        unsafe {
            let _ = device.disconnect();
        }
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
            device.reconnect()
                .map_err(|e| format!("Reconnect failed: {}", e))?;
            Ok("J2534 reconnected successfully".into())
        }
    } else {
        Err("No previous J2534 device to reconnect. Use j2534_connect_cmd instead.".into())
    }
}

// Also improve the original connect command with better error recovery
// (already updated in previous step)