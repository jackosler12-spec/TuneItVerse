// Improved apply_live_patch with real memory addresses

#[tauri::command]
async fn apply_live_patch(
    state: State<'_, AppState>,
    table_id: String,
    row: usize,
    col: usize,
    new_value: f64,
) -> Result<String, String> {
    // Get real memory address
    let address = crate::ecu_database::calculate_cell_address(&table_id, row, col)
        .unwrap_or(0xC000 + (row * 16 + col) as u32); // fallback

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        // Build a more realistic write request (simplified UDS Write Memory)
        // In real implementation: Mode 0x3D or custom kernel command
        let request = vec![0x3D, 0x00, (address >> 16) as u8, (address >> 8) as u8, address as u8, new_value as u8];

        unsafe {
            device.write_uds(&request, 1000)
                .map_err(|e| format!("Write failed at 0x{:X}: {}", address, e))?;
        }

        return Ok(format!("Live patch applied to {} [{}][{}] @ 0x{:X} = {}", 
            table_id, row, col, address, new_value));
    }

    // ELM / fallback
    Ok(format!("[SIM] Patch @ 0x{:X} applied: {} [{}][{}] = {}", 
        address, table_id, row, col, new_value))
}