#[tauri::command]
fn compare_p01_gm_seed(seed_hex: String, algo: Option<u32>, scan: Option<bool>) -> Result<String, String> {
    let a = algo.map(|v| v as u16);
    let r = crate::p01_gm_compare::compare(&seed_hex, a, scan.unwrap_or(false))?;
    Ok(serde_json::to_string(&r).unwrap_or_else(|_| "{}".into()))
}
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
#[tauri::command] fn validate_bin_checksums_summary_cmd(data: Vec<u8>) -> Result<String, String> {
    if crate::cs_guard::p01_corrector_blocked(&data) {
        let fam = if crate::cs_guard::p59_blocks_p01_corrector(&data) { "GM_P59" } else { "HONDA_KEIHIN" };
        return Ok(format!("Checksum validation for {} ({} bytes) using report-only: P01 additive blocked\n", fam, data.len()));
    }
    checksum::validate_bin_checksums_summary(&data)
}
#[tauri::command] fn validate_checksums_cmd(data: Vec<u8>) -> Result<String, String> {
    if crate::cs_guard::p01_corrector_blocked(&data) {
        let fam = if crate::cs_guard::p59_blocks_p01_corrector(&data) { "GM_P59" } else { "HONDA_KEIHIN" };
        return Ok(json!({"ecu_family":fam,"all_valid":false,"method_used":"report-only: P01 additive blocked","regions":[]}).to_string());
    }
    Ok(serde_json::to_string_pretty(&checksum::validate_checksums(&data)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command] fn correct_bin_checksums(data: Vec<u8>) -> Result<Vec<u8>, String> {
    if crate::cs_guard::p01_corrector_blocked(&data) {
        return Err("Honda or P59 OS on this image. P01 additive correction is blocked.".into());
    }
    Ok(checksum::correct_checksums(&data)?.data)
}
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
/// P01 OS 12225074 full 512KB read-back through Kernel-P01.bin (STN adapter). Saves
/// <app data>/backups/p01_<os>_<ms>.bin + .json (SHA-256, quality).
#[tauri::command]
fn p01_full_backup(app: tauri::AppHandle, dir: Option<String>) -> Result<String, String> {
    use tauri::Manager;
    let dir = match dir.filter(|d| !d.trim().is_empty()) {
        Some(d) => std::path::PathBuf::from(d),
        None => app.path().app_data_dir().map_err(|e| e.to_string())?.join("backups"),
    };
    with_port(|port| {
        let mut logs = vec![];
        let mut link = crate::p01_backup::StnLink::new(port);
        let r = (|| {
            let (img, os) = crate::p01_backup::read_full_image(&mut link, |p| { let _ = app.emit("p01-backup-progress", &p); }, &mut logs)?;
            let a = crate::p01_backup::assess_image(&img, os);
            let saved = crate::p01_backup::save_backup(&dir, &img, &a)?;
            Ok::<_, String>((saved, a, os))
        })();
        let entry = |res: &str, os: Option<u32>, sha: Option<String>, detail: String| crate::audit_log::AuditEntry {
            ts_unix_ms: crate::audit_log::now_ms(), kind: "backup".into(), family: "P01_0411".into(), os_id: os,
            image_sha256: None, backup_sha256: sha, gates_passed: vec![], gate_failed: None, result: res.into(), detail };
        match r {
            Ok((saved, a, os)) => {
                let full = a.quality == crate::flash::BackupQuality::FullImage;
                let mut e = entry(if full { "ok" } else { "failed" }, Some(os), Some(saved.sha256.clone()), format!("quality {:?}; {}", a.quality, a.reasons.join("; ")));
                if full { e.gates_passed = vec!["all bytes".into(), "OS id".into(), "8 checksums".into()]; } else { e.gate_failed = Some(a.reasons.join("; ")); }
                let _ = crate::audit_log::append(&e);
                Ok(json!({"backup": saved, "logs": logs}).to_string())
            }
            Err(err) => { let mut e = entry("failed", None, None, err.clone()); e.gate_failed = Some(err.clone()); let _ = crate::audit_log::append(&e); Err(err) }
        }
    })
}
/// Last `limit` audit log entries (newest last) plus the log path.
#[tauri::command]
fn audit_log_entries(limit: Option<usize>) -> Result<String, String> {
    let p = crate::audit_log::audit_path();
    Ok(json!({"path": p.to_string_lossy(), "entries": crate::audit_log::read_from(&p, limit.unwrap_or(200))}).to_string())
}
/// Pick the family for live compare/verify. A family requested by the UI must
/// agree with the one identified from the BIN; with no BIN match, the request
/// (then session state) is used. Nothing → explicit error, never a default.
fn pick_live_family(from_bin: Result<String, String>, requested: Option<String>, from_state: Option<String>) -> Result<String, String> {
    let req = requested.map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
    match (from_bin, req) {
        (Ok(b), Some(r)) if !b.eq_ignore_ascii_case(&r) => Err(format!(
            "Family mismatch: BIN identifies as {} but {} was requested. Refusing.", b, r
        )),
        (Ok(b), _) => Ok(b),
        (Err(_), Some(r)) => Ok(r),
        (Err(e), None) => from_state.ok_or_else(|| format!("ECU family unknown ({}). Identify the BIN or pick a family.", e)),
    }
}

fn family_for_live(file_bytes: &[u8], requested: Option<String>) -> Result<String, String> {
    let bin = crate::v29_tools::resolved_family(file_bytes);
    let state = if bin.is_err() {
        STATE.lock().ok().and_then(|g| g.last_family.clone().or_else(|| {
            g.last_os_id.clone().and_then(|os| crate::ecu_database::get_ecu_by_os_id(&os).map(|e| e.ecu_family))
        }))
    } else { None };
    pick_live_family(bin, requested, state)
}

#[tauri::command]
fn compare_bin_to_ecu(file_bytes: Vec<u8>, family: Option<String>) -> Result<String, String> {
    let fam = family_for_live(&file_bytes, family)?;
    with_port(|port| {
        let mut logs = Vec::new();
        let windows = match crate::live_verify::probe_live_windows(port, &fam, file_bytes.len(), &mut logs) {
            Ok(w) => w,
            Err(e) => return Ok(json!({"family":fam,"matched":false,"error":e,"logs":logs}).to_string()),
        };
        match crate::live_verify::compare_windows(&file_bytes, &windows, &mut logs) {
            Ok((crc, matched)) => Ok(json!({"family":fam,"windows":windows.len(),"matched":matched,"crc":format!("0x{:08X}", crc),"logs":logs}).to_string()),
            Err(e) => Ok(json!({"family":fam,"windows":windows.len(),"matched":false,"error":e,"logs":logs}).to_string()),
        }
    }).or_else(|e| Ok(json!({"success":false,"error":format!("Not connected: {}", e)}).to_string()))
}
#[tauri::command]
fn verify_after_write(expected_bytes: Option<Vec<u8>>, family: Option<String>) -> Result<String, String> {
    let data = expected_bytes.unwrap_or_default();
    if data.is_empty() { return Err("No expected image provided".into()); }
    let fam = family_for_live(&data, family)?;
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
    let fam = family.map(|f| f.trim().to_string()).filter(|f| !f.is_empty())
        .ok_or_else(|| "ECU family required for Bosch UDS unlock (no default).".to_string())?;
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
        .setup(|app| {
            use tauri::Manager;
            if let Ok(d) = app.path().app_data_dir() { crate::audit_log::set_dir(d); }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_serial_ports, get_connection_health, connect_ecu, disconnect_ecu, auto_detect_protocol,
            st_identify, st_read_voltage, st_set_protocol, st_open_protocol, st_set_timeout,
            st_clear_filters, st_set_pass_filter, st_set_flow_control_filter,
            st_start_monitor, st_stop_monitor, st_enter_sleep, st_set_sleep_wake,
            st_read_pp, st_write_pp, st_stpx,
            list_supported_protocols, list_supported_ecus, list_ecu_catalog, get_ecu_info, read_properties, read_ecu_data,
            get_logging_templates, log_get_status, log_start, log_stop, log_set_channels, log_apply_template,
            log_capture_sample, log_set_demo_mode, log_get_samples, log_clear, log_export_csv, log_import_csv,
            compute_seed_key, gm_key_table_info, compare_p01_gm_seed, read_dtcs_cmd, read_freeze_frame_cmd, clear_dtcs_cmd, dtc::explain_dtcs_cmd,
            validate_bin_checksums_summary_cmd, validate_checksums_cmd, correct_bin_checksums,
            xdf::parse_xdf_definitions, xdf::extract_table_from_bin, xdf::patch_table_into_bin, xdf::table_delta_cmd,
            a2l::parse_a2l_definitions, a2l::parse_a2l_summary,
            table_tools::table_math_cmd, table_tools::apply_stft_preview_cmd,
            auto_load_tables_for_bin, get_tuning_advice, guided_flash_pipeline, p01_full_backup, audit_log_entries, compare_bin_to_ecu, verify_after_write,
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
            v312::session_snapshot, v312::list_supported_adapters,
            defpack::import_definition_pack, defpack::list_definition_packs, defpack::operational_self_check,
            bin_inspect::bin_hex_window, bin_inspect::bin_profile, bin_inspect::bin_tune_report,
            log_studio::log_session_summary, log_studio::log_replay_frame, log_studio::log_trim_suggestion, log_studio::bin_diff_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TuneItVerse");
}

#[cfg(test)]
mod live_family_tests {
    use super::*;
    #[test]
    fn bin_family_wins_and_mismatch_refused() {
        assert_eq!(pick_live_family(Ok("P01_0411".into()), None, None).unwrap(), "P01_0411");
        assert_eq!(pick_live_family(Ok("P01_0411".into()), Some("p01_0411".into()), None).unwrap(), "P01_0411");
        assert!(pick_live_family(Ok("P01_0411".into()), Some("EDC16C41".into()), None).unwrap_err().contains("mismatch"));
    }
    #[test]
    fn unresolved_bin_uses_request_then_state_else_errors() {
        assert_eq!(pick_live_family(Err("x".into()), Some("EDC16C41".into()), Some("P01_0411".into())).unwrap(), "EDC16C41");
        assert_eq!(pick_live_family(Err("x".into()), Some("  ".into()), Some("P01_0411".into())).unwrap(), "P01_0411");
        assert!(pick_live_family(Err("x".into()), None, None).unwrap_err().contains("unknown"));
    }
    #[test]
    fn bosch_unlock_requires_family() {
        assert!(bosch_uds_unlock(None, None).unwrap_err().contains("family required"));
    }
}
