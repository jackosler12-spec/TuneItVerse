// Proper UDS Mode 0x3D implementation

/// Builds a UDS Mode 0x3D (Write Memory by Address) request
/// Format: [0x3D] [AddressAndLengthIdentifier] [Address...] [MemorySize] [Data...]
fn build_uds_write_memory_request(address: u32, data: &[u8]) -> Vec<u8> {
    let mut request = Vec::new();
    request.push(0x3D); // SID: Write Memory by Address

    // AddressAndLengthIdentifier (common simple format)
    // Bit 7-4: memorySize length (1 byte)
    // Bit 3-0: memoryAddress length (4 bytes = 32-bit)
    let alid: u8 = 0x14; // 1 byte size + 4 byte address
    request.push(alid);

    // Memory Address (big endian, 32-bit)
    request.push((address >> 24) as u8);
    request.push((address >> 16) as u8);
    request.push((address >> 8) as u8);
    request.push(address as u8);

    // Memory Size (number of bytes to write)
    request.push(data.len() as u8);

    // Data to write
    request.extend_from_slice(data);

    request
}

// Update apply_live_patch to use the real Mode 3D builder

#[tauri::command]
async fn apply_live_patch(
    state: State<'_, AppState>,
    table_id: String,
    row: usize,
    col: usize,
    new_value: f64,
) -> Result<String, String> {
    let address = crate::ecu_database::calculate_cell_address(&table_id, row, col)
        .unwrap_or(0x0000C000 + (row * 16 + col) as u32);

    // Convert value to byte(s) - for now assume 1-byte tables
    let data_to_write: Vec<u8> = vec![new_value.clamp(0.0, 255.0) as u8];

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        let request = build_uds_write_memory_request(address, &data_to_write);

        unsafe {
            device.write_uds(&request, 1500)
                .map_err(|e| format!("UDS Mode 3D write failed at 0x{:08X}: {}", address, e))?;
        }

        return Ok(format!(
            "UDS 0x3D Write successful: {} [{}][{}] @ 0x{:08X} = {}",
            table_id, row, col, address, new_value
        ));
    }

    // Fallback for ELM / simulation
    Ok(format!(
        "[SIMULATED] Mode 3D Write @ 0x{:08X}: {} [{}][{}] = {}",
        address, table_id, row, col, new_value
    ))
}