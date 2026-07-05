// lib.rs — Added read_live_data command

// ... existing code ...

// ==================== LIVE DATA ====================

#[tauri::command]
fn read_live_data(
    state: State<'_, AppState>,
    pids: Vec<String>,
) -> Result<serde_json::Value, String> {
    let mut result = serde_json::json!({});

    let guard = state.j2534_device.lock().map_err(|e| e.to_string())?;

    if let Some(ref device) = *guard {
        // Real J2534 path (UDS Mode 01 for standard PIDs)
        for pid_str in pids {
            let pid: u8 = match pid_str.as_str() {
                "rpm" => 0x0C,
                "map" => 0x0B,
                "afr" => 0x34,      // Example - may need Mode 22 on some ECUs
                "boost" => 0x0B,    // MAP as proxy
                _ => continue,
            };

            // Simple Mode 01 request
            let request = vec![0x01, pid];
            unsafe {
                if let Err(_) = device.write_uds(&request, 500) {
                    continue;
                }
                if let Ok(msgs) = device.read_uds(300, 3) {
                    for msg in msgs {
                        if msg.DataSize > 3 && msg.Data[0] == 0x41 && msg.Data[1] == pid {
                            let value = match pid {
                                0x0C => ((msg.Data[2] as u16) << 8 | msg.Data[3] as u16) as f64 / 4.0, // RPM
                                0x0B => msg.Data[2] as f64 * 0.5, // MAP kPa (approx)
                                _ => msg.Data[2] as f64,
                            };
                            result[pid_str.clone()] = serde_json::json!(value);
                            break;
                        }
                    }
                }
            }
        }
    } else {
        // Mock data for development / when no J2534
        for pid in pids {
            let value: f64 = match pid.as_str() {
                "rpm" => 2450.0 + (rand::random::<f64>() * 300.0),
                "map" => 85.0 + (rand::random::<f64>() * 25.0),
                "afr" => 14.2 + (rand::random::<f64>() * 0.6),
                "boost" => 0.85 + (rand::random::<f64>() * 0.3),
                _ => 0.0,
            };
            result[pid] = serde_json::json!(value);
        }
    }

    Ok(result)
}