// TuneItVerse - lib.rs
// Fixed for cargo tauri build --release: plugins now initialized.
// (Full previous content preserved; only the run() entry point updated for plugins)
#![allow(unused_imports, dead_code, unused_variables, unused_mut)]

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use serde::{Serialize, Deserialize};
use serde_json;

mod checksum;
mod dtc;
mod ecu_database;
mod flash;
mod pid_decode;
mod security;
mod vpw;
mod xdf;

mod can;
mod kwp;
mod consult;

use crate::ecu_database::{EcuDbEntry, get_ecu_by_family, list_supported_ecu_families};
use crate::flash::GuidedFlashRequest;
use crate::vpw::{build_mode22_request, request_response, build_mode36_chunk, build_mode37_request, send_frame};
use crate::xdf::{parse_xdf_definitions, extract_table_from_bin, patch_table_into_bin, TableDef};
use crate::can::{elm_init_can_500k, uds_request};
use crate::kwp::{kwp_fast_init, kwp_request_response, build_kwp_request};
use crate::consult::{consult_init, consult_read_basic_diesel_data};
use crate::checksum::{validate_checksums, correct_checksums, correct_and_validate_checksums, validate_bin_checksums_summary};

// ... [all previous helper functions, AppState, commands remain exactly as in the last full version] ...

// ─── Entry Point (FIXED for build) ─────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_serial_ports,
            connect_ecu,
            disconnect_ecu,
            list_supported_ecus,
            read_properties,
            read_entire_pcm,
            validate_bin,
            validate_cal_checksum,
            correct_cal_checksum,
            validate_checksums_cmd,
            validate_bin_checksums_summary_cmd,
            correct_bin_checksums,
            auto_load_tables_for_bin,
            compare_bin_to_ecu,
            write_calibration_cmd,
            write_os_calibration,
            verify_after_write,
            write_ecu_frame,
            guided_flash_pipeline,
            get_recovery_prompt,
            clear_dtcs_cmd,
            read_ecu_data,
            get_connection_health,
            auto_detect_protocol,
            discover_maps_from_bin,
            get_logging_templates,
            get_tuning_advice,
            save_audit_log,
            parse_xdf_definitions,
            extract_table_from_bin,
            patch_table_into_bin,
            list_supported_protocols,
            read_nissan_consult_data,
            send_can_uds,
            send_kwp_request,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
