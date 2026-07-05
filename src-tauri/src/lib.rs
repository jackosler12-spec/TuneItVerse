// Updated enter_diagnostic_session to track state

fn enter_diagnostic_session(device: &crate::j2534::J2534Device, session_type: u8, state: &State<'_, AppState>) -> Result<(), String> {
    let request = vec![0x10, session_type];
    unsafe {
        device.write_uds(&request, 800)?;
        let responses = device.read_uds(600, 2)?;
        if responses.is_empty() || responses[0].Data[0] != 0x50 {
            return Err(format!("Failed to enter session 0x{:02X}", session_type));
        }
    }
    update_connection_state(state, Some(session_type), None, None, None);
    Ok(())
}

// Updated perform_security_access_cmd to track state

#[tauri::command]
async fn perform_security_access_cmd(
    state: State<'_, AppState>,
    level: u8,
    family: String,
) -> Result<String, String> {
    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        unsafe {
            crate::security::perform_security_access(device, level, &family)?;
        }
        update_connection_state(&state, None, Some(true), Some(level), Some(family));
        return Ok(format!("Security Access Level {} successful", level));
    }
    Err("No J2534 device connected".into())
}

// Smarter apply_live_patch with state checking

#[tauri::command]
async fn apply_live_patch(
    state: State<'_, AppState>,
    table_id: String,
    row: usize,
    col: usize,
    new_value: f64,
    auto_unlock: Option<bool>,
    family: Option<String>,
    session_type: Option<u8>,
) -> Result<String, String> {
    let do_unlock = auto_unlock.unwrap_or(true);
    let ecu_family = family.unwrap_or_else(|| "P01".to_string());
    let desired_session = session_type.unwrap_or(0x02);

    // Check current state
    let current_state = {
        state.connection_state.lock().map_err(|e| e.to_string())?.clone()
    };

    let address = crate::ecu_database::calculate_cell_address(&table_id, row, col)
        .unwrap_or(0x0000C000 + (row * 16 + col) as u32);

    let data_to_write: Vec<u8> = vec![new_value.clamp(0.0, 255.0) as u8];

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        // Only enter session if not already in it
        if current_state.current_session != Some(desired_session) {
            if let Err(e) = enter_diagnostic_session(device, desired_session, &state) {
                eprintln!("Session warning: {}", e);
            }
        }

        // Only do security if not already unlocked
        if do_unlock && !current_state.security_unlocked {
            unsafe {
                if let Err(e) = crate::security::perform_security_access(device, 1, &ecu_family) {
                    eprintln!("Security warning: {}", e);
                } else {
                    update_connection_state(&state, None, Some(true), Some(1), Some(ecu_family.clone()));
                }
            }
        }

        // Perform the write
        let request = build_uds_write_memory_request(address, &data_to_write);

        unsafe {
            device.write_uds(&request, 1500)?;
        }

        return Ok(format!(
            "Live patch successful @ 0x{:08X} (session=0x{:02X})",
            address, desired_session
        ));
    }

    Ok("[SIM] Patch applied".into())
}