// TuneItVerse lib.rs — Tauri entry + command surface (v3.30.0)
#![allow(unused_imports, dead_code, non_snake_case)]

mod a2l;
mod can;
mod checksum;
mod checksum_sizes;
mod consult;
mod cs_guard;
mod dtc;
mod ecu_database;
mod file_dialog;
mod flash;
mod j2534;
mod j2534_list;
mod kwp;
mod live_verify;
mod logging;
mod pid_decode;
mod scripting;
mod security;
mod seed_tables;
mod gm_keys;
mod p01_gm_compare;
mod table_tools;
mod tableseek;
mod uds;
mod vpw;
mod xdf;
mod v29_tools;
mod transport;
mod v312;

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

include!("commands_p1.rs");
include!("commands_p2.rs");
include!("commands_p3.rs");
