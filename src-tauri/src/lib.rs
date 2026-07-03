// ... existing code ...

// ISO-TP Configuration & Stats Commands
#[tauri::command]
fn set_iso_tp_parameters(block_size: u8, stmin_ms: u64) -> Result<String, String> {
    crate::can::set_iso_tp_config(block_size, stmin_ms);
    Ok(format!("ISO-TP config updated: BS={}, STmin={}ms", block_size, stmin_ms))
}

#[tauri::command]
fn get_iso_tp_statistics() -> Result<String, String> {
    let stats = crate::can::get_iso_tp_stats();
    serde_json::to_string(&stats).map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_iso_tp_statistics() -> Result<String, String> {
    crate::can::reset_iso_tp_stats();
    Ok("ISO-TP statistics reset".into())
}

// Enhanced guided_flash_pipeline with deeper ISO-TP logging
#[tauri::command]
async fn guided_flash_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    request_json: String,
) -> Result<String, String> {
    let request: GuidedFlashRequest = serde_json::from_str(&request_json)
        .map_err(|e| format!("Invalid request: {}", e))?;

    let _ = app.emit("flash-log", "Starting guided flash pipeline...");
    let _ = app.emit("flash-log", format!("Using ISO-TP config: BS={}, STmin={}ms", 
        crate::can::get_iso_tp_config().block_size,
        crate::can::get_iso_tp_config().stmin_ms));

    // ... existing pipeline logic ...

    // Example deeper logging during kernel upload
    let _ = app.emit("flash-log", "[ISO-TP] Sending kernel via First Frame + Flow Control...");
    // (actual iso_tp_send call happens inside write_ecu_frame or flash module)

    let result = flash::orchestrate_guided_flash(...).map_err(|e| e.to_string())?;

    let stats = crate::can::get_iso_tp_stats();
    let _ = app.emit("flash-log", format!("[ISO-TP] Pipeline complete. FF sent: {}, CF sent: {}, Bytes: {}", 
        stats.ff_sent, stats.cf_sent, stats.bytes_sent));

    // ... rest of function ...
}