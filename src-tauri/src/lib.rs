// Automatic Diagnostic Session Control (Mode 0x10)

/// Enter a diagnostic session (usually 0x02 = Extended)
fn enter_diagnostic_session(device: &crate::j2534::J2534Device, session_type: u8) -> Result<(), String> {
    let request = vec![0x10, session_type];
    unsafe {
        device.write_uds(&request, 800)?;
        let responses = device.read_uds(600, 2)?;
        if responses.is_empty() || responses[0].Data[0] != 0x50 {
            return Err(format!("Failed to enter session 0x{:02X}", session_type));
        }
    }
    Ok(())
}

// Update apply_live_patch to include automatic session control

#[tauri::command]
async fn apply_live_patch(
    state: State<'_, AppState>,
    table_id: String,
    row: usize,
    col: usize,
    new_value: f64,
    auto_unlock: Option<bool>,
    family: Option<String>,
    session_type: Option<u8>,   // New parameter
) -> Result<String, String> {
    let do_unlock = auto_unlock.unwrap_or(true);
    let ecu_family = family.unwrap_or_else(|| "P01".to_string());
    let session = session_type.unwrap_or(0x02); // Default to Extended Diagnostic Session

    let address = crate::ecu_database::calculate_cell_address(&table_id, row, col)
        .unwrap_or(0x0000C000 + (row * 16 + col) as u32);

    let data_to_write: Vec<u8> = vec![new_value.clamp(0.0, 255.0) as u8];

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        // 1. Automatic Session Control
        if let Err(e) = enter_diagnostic_session(device, session) {
            eprintln!("Session control warning: {}", e);
        }

        // 2. Automatic Security Access
        if do_unlock {
            unsafe {
                if let Err(e) = crate::security::perform_security_access(device, 1, &ecu_family) {
                    eprintln!("Security unlock warning: {}", e);
                }
            }
        }

        // 3. Mode 0x3D Write
        let request = build_uds_write_memory_request(address, &data_to_write);

        unsafe {
            device.write_uds(&request, 1500)
                .map_err(|e| format!("Mode 3D write failed: {}", e))?;
        }

        return Ok(format!(
            "Live patch successful (session=0x{:02X}, unlock={}) : {} [{}][{}] @ 0x{:08X} = {}",
            session, do_unlock, table_id, row, col, address, new_value
        ));
    }

    Ok(format!("[SIM] Patch applied"))
}