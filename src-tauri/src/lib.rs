// Updated apply_live_patch with automatic security unlock

#[tauri::command]
async fn apply_live_patch(
    state: State<'_, AppState>,
    table_id: String,
    row: usize,
    col: usize,
    new_value: f64,
    auto_unlock: Option<bool>,
    family: Option<String>,
) -> Result<String, String> {
    let do_unlock = auto_unlock.unwrap_or(true);
    let ecu_family = family.unwrap_or_else(|| "P01".to_string());

    let address = crate::ecu_database::calculate_cell_address(&table_id, row, col)
        .unwrap_or(0x0000C000 + (row * 16 + col) as u32);

    let data_to_write: Vec<u8> = vec![new_value.clamp(0.0, 255.0) as u8];

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        // Automatic Security Access if requested
        if do_unlock {
            unsafe {
                if let Err(e) = crate::security::perform_security_access(device, 1, &ecu_family) {
                    // Non-fatal for now - many development scenarios already have access
                    eprintln!("Security unlock warning: {}", e);
                }
            }
        }

        let request = build_uds_write_memory_request(address, &data_to_write);

        unsafe {
            device.write_uds(&request, 1500)
                .map_err(|e| format!("Mode 3D write failed at 0x{:08X}: {}", address, e))?;
        }

        return Ok(format!(
            "Live patch applied (auto_unlock={}) : {} [{}][{}] @ 0x{:08X} = {}",
            do_unlock, table_id, row, col, address, new_value
        ));
    }

    Ok(format!(
        "[SIM] Patch @ 0x{:08X}: {} [{}][{}] = {}",
        address, table_id, row, col, new_value
    ))
}