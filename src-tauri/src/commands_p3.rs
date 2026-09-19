#[tauri::command] fn read_dtcs_cmd() -> Result<String, String> {
    with_port(|port| dtc::read_dtcs(port).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{}".into())))
        .or_else(|_| Ok(json!({"stored":[],"pending":[],"permanent":[],"total":0}).to_string()))
}
#[tauri::command] fn read_freeze_frame_cmd() -> Result<String, String> {
    with_port(|port| dtc::read_freeze_frame(port).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{}".into()))).or_else(|_| Ok("{}".into()))
}
#[tauri::command] fn clear_dtcs_cmd() -> Result<String, String> {
    with_port(|port| dtc::clear_dtcs(port, 0).map(|r| serde_json::to_string(&r).unwrap_or_else(|_| "{\"success\":true}".into())))
        .or_else(|_| Ok(json!({"success":false,"message":"DTC clear refused offline. Connect an adapter."}).to_string()))
}
#[tauri::command] fn validate_bin_checksums_summary_cmd(data: Vec<u8>) -> Result<String, String> { checksum::validate_bin_checksums_summary(&data) }
#[tauri::command] fn validate_checksums_cmd(data: Vec<u8>) -> Result<String, String> { Ok(serde_json::to_string_pretty(&checksum::validate_checksums(&data)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn correct_bin_checksums(data: Vec<u8>) -> Result<Vec<u8>, String> { Ok(checksum::correct_checksums(&data)?.data) }
#[tauri::command] fn auto_load_tables_for_bin(bin_bytes: Vec<u8>) -> Result<String, String> {
    Ok(serde_json::to_string(&ecu_database::tables_for_bin(&bin_bytes)).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command] fn get_tuning_advice(table_id: String, sample_value: f64, ecu_family: String) -> Result<String, String> {
    Ok(format!(
        "{} on {}. Sample cell {:.4}. Description lives on the Maps details pane from the TableSeek pack — this command does not invent a tune.",
        table_id, ecu_family, sample_value
    ))
}
#[tauri::command]
fn guided_flash_pipeline(app: tauri::AppHandle, request_json: String) -> Result<String, String> {
    let request: flash::GuidedFlashRequest = serde_json::from_str(&request_json).map_err(|e| format!("Invalid GuidedFlashRequest: {}", e))?;
    with_port(|port| {
        let result = flash::orchestrate_guided_flash(port, request, |p| {
            let _ = app.emit("flash-progress", &p);
        })?;
        Ok(serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()))
    }).or_else(|e| Ok(json!({"success":false,"steps_completed":[],"logs":[format!("Fail-closed: not connected ({})", e)],"verified_live":false,"error":format!("Not connected: {}", e)}).to_string()))
}
fn family_from_bin_or_state(file_bytes: &[u8]) -> Result<String, String> {
    match crate::v29_tools::resolved_family(file_bytes) {
        Ok(f) => {
            if let Ok(mut guard) = STATE.lock() {
                guard.last_family = Some(f.clone());
            }
            Ok(f)
        }
        Err(ident_err) => {
            let guard = STATE.lock().map_err(|e| e.to_string())?;
            if let Some(f) = guard.last_family.clone() { return Ok(f); }
            if let Some(os) = guard.last_os_id.clone() {
                if let Some(e) = crate::ecu_database::get_ecu_by_os_id(&os) {
                    return Ok(e.ecu_family);
                }
            }
            Err(format!("Family unresolved: {}", ident_err))
        }
    }
}

#[tauri::command]
fn compare_bin_to_ecu(file_bytes: Vec<u8>) -> Result<String, String> {
    let fam = family_from_bin_or_state(&file_bytes)?;
    with_port(|port| {
        let mut logs = Vec::new();
        let windows = crate::live_verify::probe_live_windows(port, &fam, file_bytes.len(), &mut logs);
        match crate::live_verify::compare_windows(&file_bytes, &windows, &mut logs) {
            Ok((crc, matched)) => Ok(json!({"family":fam,"windows":windows.len(),"matched":matched,"crc":format!("0x{:08X}", crc),"logs":logs}).to_string()),
            Err(e) => Ok(json!({"family":fam,"windows":windows.len(),"matched":false,"error":e,"logs":logs}).to_string()),
        }
    }).or_else(|e| Ok(json!({"success":false,"error":format!("Not connected: {}", e)}).to_string()))
}
#[tauri::command]
fn verify_after_write(expected_bytes: Option<Vec<u8>>) -> Result<String, String> {
    let data = expected_bytes.unwrap_or_default();
    if data.is_empty() { return Err("No expected image provided".into()); }
    let fam = family_from_bin_or_state(&data)?;
    with_port(|port| {
        match flash::verify_after_write(port, &fam, &data, &mut vec![]) {
            Ok((crc, matched)) => Ok(format!("Live CRC 0x{:08X} matched={} family={}", crc, matched, fam)),
            Err(e) => Ok(format!("Verify note: {}", e)),
        }
    }).or_else(|_| Ok("Not connected".into()))
}
#[tauri::command] fn unlock_level1() -> Result<String, String> { with_port(|port| Ok(serde_json::to_string(&security::unlock_level1(port)?).unwrap_or_else(|_| "{}".into()))) }
#[tauri::command] fn unlock_level2() -> Result<String, String> { with_port(|port| Ok(serde_json::to_string(&security::unlock_level2(port)?).unwrap_or_else(|_| "{}".into()))) }
#[tauri::command]
fn bosch_uds_unlock(family: Option<String>, level: Option<String>) -> Result<String, String> {
    let fam = family.unwrap_or_else(|| "EDC16C41".into());
    let lvl = security::BoschSecurityLevel::from_str(&level.unwrap_or_else(|| "programming".into()));
    with_port(|port| security::bosch_uds_unlock_full(port, &fam, lvl))
        .or_else(|e| Ok(json!({"success":false,"message":"Bosch UDS unlock refused offline. Connect an adapter.","family":fam,"error":e}).to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_serial_ports, get_connection_health, connect_ecu, disconnect_ecu, auto_detect_protocol,
            list_supported_protocols, list_supported_ecus, list_ecu_catalog, get_ecu_info, read_properties, read_ecu_data,
            get_logging_templates, log_get_status, log_start, log_stop, log_set_channels, log_apply_template,
            log_capture_sample, log_get_samples, log_clear, log_export_csv, log_import_csv,
            compute_seed_key, gm_key_table_info, read_dtcs_cmd, read_freeze_frame_cmd, clear_dtcs_cmd,
            validate_bin_checksums_summary_cmd, validate_checksums_cmd, correct_bin_checksums,
            xdf::parse_xdf_definitions, xdf::extract_table_from_bin, xdf::patch_table_into_bin,
            a2l::parse_a2l_definitions, a2l::parse_a2l_summary,
            table_tools::table_math_cmd, table_tools::apply_stft_preview_cmd,
            auto_load_tables_for_bin, get_tuning_advice, guided_flash_pipeline, compare_bin_to_ecu, verify_after_write,
            unlock_level1, unlock_level2, bosch_uds_unlock,
            scripting::list_script_helpers, scripting::run_bench_script,
            v29_tools::identify_bin_cmd, v29_tools::compare_bins_cmd, v29_tools::map_from_log_cmd, v29_tools::export_workspace_cmd, v29_tools::import_workspace_cmd, v29_tools::patch_bin_bytes_cmd,
            file_dialog::dialog_open_file, file_dialog::dialog_save_bytes, file_dialog::dialog_save_text,
            cs_guard::scan_checksum_candidates_cmd,
            j2534_list::j2534_list_devices, j2534::j2534_connect, j2534::j2534_connect_vpw, j2534::j2534_disconnect,
            j2534::j2534_write, j2534::j2534_read, j2534::j2534_set_data_rate,
            j2534::j2534_set_vpw_high_speed, j2534::j2534_set_vpw_normal_speed,
            j2534::j2534_read_vbatt, j2534::j2534_set_iso15765_timing, j2534::j2534_clear_buffers,
            v312::app_info, v312::read_battery_voltage_cmd, v312::correct_bin_checksums_report,
            v312::session_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TuneItVerse");
}
