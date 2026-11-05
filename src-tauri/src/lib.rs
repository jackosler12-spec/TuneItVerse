// TuneItVerse lib.rs — Tauri entry + command surface (v3.60.0)
#![allow(unused_imports, dead_code, non_snake_case)]

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

mod a2l;
mod bin_inspect;
mod can;
mod checksum;
mod checksum_sizes;
mod consult;
mod cs_guard;
mod defpack;
mod dtc;
mod ecu_database;
mod file_dialog;
mod flash;
mod j2534;
mod j2534_list;
mod kwp;
mod live_verify;
mod log_studio;
mod live_dashboard;
mod logging;
mod pid_decode;
mod scripting;
mod security;
mod seed_tables;
mod gm_keys;
mod gm_osid;
mod p01_gm_compare;
mod table_tools;
mod transfer_cancel;
mod tableseek;
mod uds;
mod vpw;
mod xdf;
mod v29_tools;
mod private_bin_probe;
mod elm_parse;
mod replay_port;
mod transport;
mod v312;
mod voltage_watch;
mod p01_backup;
mod p01_write;
mod audit_log;

use tauri::Emitter;

use serialport::SerialPort;
use std::sync::Mutex;
use std::time::Duration;
use serde_json::json;
use std::collections::HashMap;

pub(crate) struct AppState {
    port: Option<Box<dyn SerialPort + Send>>,
    protocol: String,
    last_os_id: Option<String>,
    last_family: Option<String>,
}

pub(crate) static STATE: Mutex<AppState> = Mutex::new(AppState { port: None, protocol: String::new(), last_os_id: None, last_family: None });

pub(crate) fn with_port<F, R>(f: F) -> Result<R, String>
where F: FnOnce(&mut Box<dyn SerialPort + Send>) -> Result<R, String>,
{
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    match guard.port.as_mut() {
        Some(p) => f(p),
        None => Err("Not connected. Call connect_ecu first.".into()),
    }
}

/// True when the open serial session is a scripted ELM trace, not an adapter.
pub(crate) fn replay_session() -> bool {
    STATE.lock().map(|g| g.protocol.eq_ignore_ascii_case("replay")).unwrap_or(false)
}

/// Fail closed before any write command can use the scripted port.
pub(crate) fn refuse_replay(action: &str) -> Result<(), String> {
    if replay_session() {
        Err(format!("{action} refused: replay fixture is not a vehicle."))
    } else {
        Ok(())
    }
}

/// Serializes tests that connect or expect a disconnected port.
#[cfg(test)]
pub(crate) static TEST_PORT_LOCK: Mutex<()> = Mutex::new(());

include!("commands_p1.rs");
include!("commands_p2.rs");
include!("commands_p3.rs");
