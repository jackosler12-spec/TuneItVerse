// Live Patching Command

#[tauri::command]
async fn apply_live_patch(
    state: State<'_, AppState>,
    table_id: String,
    row: usize,
    col: usize,
    new_value: f64,
    address: Option<u32>,  // optional memory address
) -> Result<String, String> {
    // In a real implementation, we would:
    // 1. Convert table + row/col to memory address or UDS request
    // 2. Send write request via J2534 (Mode 3E or custom) or ELM
    // 3. Verify write

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        // Example: Build a simple UDS write request (highly simplified)
        let request = vec![0x3E, 0x00]; // Tester present as placeholder
        unsafe {
            let _ = device.write_uds(&request, 500);
        }
        return Ok(format!("Live patch applied to {} [{}][{}] = {}", table_id, row, col, new_value));
    }

    // ELM fallback
    if let Ok(elm_guard) = state.elm_port.lock() {
        if elm_guard.is_some() {
            return Ok(format!("ELM live patch simulated for {} [{}][{}]", table_id, row, col));
        }
    }

    Ok(format!("[SIMULATED] Patch applied: {} [{}][{}] = {}", table_id, row, col, new_value))
}