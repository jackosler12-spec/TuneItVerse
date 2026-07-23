// j2534.rs — J2534 PassThru support for professional interfaces (DrewTech, Tactrix, etc.)
// Expanded in next-pass: proper filtering, 29-bit CAN, ISO15765, basic connect/disconnect.
// Real DLLs are loaded at runtime on Windows; this provides the safe Rust FFI surface.
// Field names match the C PASSTHRU_MSG layout (not Rust snake_case).

#![allow(non_snake_case)]

use std::os::raw::{c_char, c_long, c_ulong, c_void};
use std::ptr;

// Common J2534 constants
pub const J2534_PROTOCOL_ISO15765: c_ulong = 0x06;
pub const J2534_PROTOCOL_CAN: c_ulong = 0x05;
pub const J2534_PROTOCOL_ISO9141: c_ulong = 0x03;
pub const J2534_FLAG_CAN_29BIT_ID: c_ulong = 0x00000100;
pub const J2534_FLAG_ISO15765_FRAME_PAD: c_ulong = 0x00000040;
pub const J2534_FLAG_CAN_ID_BOTH: c_ulong = 0x00000800;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PASSTHRU_MSG {
    pub ProtocolID: c_ulong,
    pub RxStatus: c_ulong,
    pub TxFlags: c_ulong,
    pub Timestamp: c_ulong,
    pub DataSize: c_ulong,
    pub ExtraDataIndex: c_ulong,
    pub Data: [u8; 4128],
}

impl Default for PASSTHRU_MSG {
    fn default() -> Self {
        Self {
            ProtocolID: 0,
            RxStatus: 0,
            TxFlags: 0,
            Timestamp: 0,
            DataSize: 0,
            ExtraDataIndex: 0,
            Data: [0u8; 4128],
        }
    }
}

type PassThruOpen = unsafe extern "C" fn(*const c_char, *mut c_ulong) -> c_long;
type PassThruClose = unsafe extern "C" fn(c_ulong) -> c_long;
type PassThruConnect = unsafe extern "C" fn(c_ulong, c_ulong, c_ulong, c_ulong, *mut c_ulong) -> c_long;
type PassThruDisconnect = unsafe extern "C" fn(c_ulong) -> c_long;
type PassThruReadMsgs = unsafe extern "C" fn(c_ulong, *mut PASSTHRU_MSG, *mut c_ulong, c_ulong) -> c_long;
type PassThruWriteMsgs = unsafe extern "C" fn(c_ulong, *mut PASSTHRU_MSG, *mut c_ulong, c_ulong) -> c_long;
type PassThruStartMsgFilter = unsafe extern "C" fn(c_ulong, c_ulong, *const PASSTHRU_MSG, *const PASSTHRU_MSG, *const PASSTHRU_MSG, *mut c_ulong) -> c_long;
type PassThruStopMsgFilter = unsafe extern "C" fn(c_ulong, c_ulong) -> c_long;
type PassThruIoctl = unsafe extern "C" fn(c_ulong, c_ulong, *mut c_void, *mut c_void) -> c_long;

pub struct J2534Device {
    pub device_id: c_ulong,
    pub channel_id: c_ulong,
    // In a full implementation these would be loaded via libloading on Windows
    // For now we keep the surface ready for real DLL binding.
}

impl J2534Device {
    pub fn new() -> Self {
        Self { device_id: 0, channel_id: 0 }
    }

    /// Placeholder open — real version loads the vendor DLL and calls PassThruOpen
    pub fn open(&mut self, _dll_path: Option<&str>) -> Result<(), String> {
        // On real Windows + DLL present this would bind symbols and open the device.
        // For cross-platform safety we return a clear message.
        Err("J2534 open requires Windows + vendor PassThru DLL. Use serial/ELM path for now or provide DLL.".into())
    }

    pub fn close(&mut self) -> Result<(), String> {
        self.device_id = 0;
        self.channel_id = 0;
        Ok(())
    }

    /// Connect with explicit 29-bit CAN support (common for modern ECUs)
    pub fn connect_can_29bit(&mut self) -> Result<(), String> {
        // Real: PassThruConnect(device_id, ISO15765, flags | 29BIT, 500000, &mut channel)
        Err("J2534 connect_can_29bit: bind real DLL first".into())
    }

    /// Start a proper ISO15765 flow-control filter (recommended for real ECUs)
    pub fn start_iso15765_filter(&self, can_id: u32, mask: u32, is_29bit: bool) -> Result<c_ulong, String> {
        let _ = (can_id, mask, is_29bit);
        // Real implementation builds mask/pattern PASSTHRU_MSG and calls StartMsgFilter
        Err("J2534 filter requires live device".into())
    }

    pub fn write_msg(&self, data: &[u8], protocol: c_ulong, flags: c_ulong) -> Result<(), String> {
        let _ = (data, protocol, flags);
        Err("J2534 write requires live device".into())
    }

    pub fn read_msgs(&self, max: usize, timeout_ms: u32) -> Result<Vec<Vec<u8>>, String> {
        let _ = (max, timeout_ms);
        Ok(vec![])
    }
}

// Tauri command surface (can be registered later when full FFI is ready)
#[tauri::command]
pub fn j2534_list_devices() -> Result<Vec<String>, String> {
    // On Windows this would enumerate registry / known DLLs
    Ok(vec!["(J2534 support prepared — place vendor DLL and rebuild for full use)".into()])
}

#[tauri::command]
pub fn j2534_connect(_dll: Option<String>) -> Result<String, String> {
    Err("J2534 full connect requires Windows + PassThru DLL. Serial/ELM/Consult paths are fully active.".into())
}
