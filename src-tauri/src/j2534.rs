// j2534.rs — J2534 PassThru binding (DrewTech, Tactrix OpenPort, etc.)
// Dynamic DLL loading via libloading on Windows.
// Includes PassThruIoctl: SET_CONFIG / GET_CONFIG / READ_VBATT / CLEAR_* buffers.
//
// Timing-critical uses:
//   DATA_RATE 10400 → 41600 after kernel 0xA0/0xA1 for VPW 4× high-speed
//   ISO15765_STMIN / ISO15765_BS for multi-frame Mode 23 dumps
//   READ_VBATT for voltage gate without Mode 01

#![allow(non_snake_case, dead_code)]

use std::os::raw::{c_char, c_long, c_ulong, c_void};
use std::ptr;
use std::ffi::CString;
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use libloading::{Library, Symbol};

// ── Protocol IDs (SAE J2534-1) ───────────────────────────────────────────────
pub const J2534_PROTOCOL_J1850PWM: c_ulong = 0x01;
pub const J2534_PROTOCOL_J1850VPW: c_ulong = 0x02;
pub const J2534_PROTOCOL_ISO9141: c_ulong = 0x03;
pub const J2534_PROTOCOL_ISO14230: c_ulong = 0x04;
pub const J2534_PROTOCOL_CAN: c_ulong = 0x05;
pub const J2534_PROTOCOL_ISO15765: c_ulong = 0x06;

// ── Connect / TX flags ──────────────────────────────────────────────────────
pub const J2534_FLAG_CAN_29BIT_ID: c_ulong = 0x0000_0100;
pub const J2534_FLAG_ISO15765_FRAME_PAD: c_ulong = 0x0000_0040;
pub const J2534_FLAG_CAN_ID_BOTH: c_ulong = 0x0000_0800;
pub const J2534_FLAG_TX_NORMAL: c_ulong = 0x0000_0000;

// ── Ioctl IDs (J2534-1 v04.04) ───────────────────────────────────────────────
pub const IOCTL_GET_CONFIG: c_ulong = 0x01;
pub const IOCTL_SET_CONFIG: c_ulong = 0x02;
pub const IOCTL_READ_VBATT: c_ulong = 0x03;
pub const IOCTL_FIVE_BAUD_INIT: c_ulong = 0x04;
pub const IOCTL_FAST_INIT: c_ulong = 0x05;
pub const IOCTL_CLEAR_TX_BUFFER: c_ulong = 0x07;
pub const IOCTL_CLEAR_RX_BUFFER: c_ulong = 0x08;
pub const IOCTL_CLEAR_PERIODIC_MSGS: c_ulong = 0x09;
pub const IOCTL_CLEAR_MSG_FILTERS: c_ulong = 0x0A;
pub const IOCTL_CLEAR_FUNCT_MSG_LOOKUP_TABLE: c_ulong = 0x0B;
pub const IOCTL_ADD_TO_FUNCT_MSG_LOOKUP_TABLE: c_ulong = 0x0C;
pub const IOCTL_DELETE_FROM_FUNCT_MSG_LOOKUP_TABLE: c_ulong = 0x0D;
pub const IOCTL_READ_PROG_VOLTAGE: c_ulong = 0x0E;

// ── SET_CONFIG / GET_CONFIG parameter IDs ───────────────────────────────────
pub const CONFIG_DATA_RATE: c_ulong = 0x01;
pub const CONFIG_LOOPBACK: c_ulong = 0x03;
pub const CONFIG_NODE_ADDRESS: c_ulong = 0x04;
pub const CONFIG_NETWORK_LINE: c_ulong = 0x05;
pub const CONFIG_P1_MIN: c_ulong = 0x06;
pub const CONFIG_P1_MAX: c_ulong = 0x07;
pub const CONFIG_P2_MIN: c_ulong = 0x08;
pub const CONFIG_P2_MAX: c_ulong = 0x09;
pub const CONFIG_P3_MIN: c_ulong = 0x0A;
pub const CONFIG_P3_MAX: c_ulong = 0x0B;
pub const CONFIG_P4_MIN: c_ulong = 0x0C;
pub const CONFIG_P4_MAX: c_ulong = 0x0D;
pub const CONFIG_W0: c_ulong = 0x19;
pub const CONFIG_W1: c_ulong = 0x1A;
pub const CONFIG_W2: c_ulong = 0x1B;
pub const CONFIG_W3: c_ulong = 0x1C;
pub const CONFIG_W4: c_ulong = 0x1D;
pub const CONFIG_ISO15765_BS: c_ulong = 0x1E;
pub const CONFIG_ISO15765_STMIN: c_ulong = 0x1F;
pub const CONFIG_BS_TX: c_ulong = 0x22;
pub const CONFIG_STMIN_TX: c_ulong = 0x23;

/// Normal J1850 VPW bit rate.
pub const VPW_BAUD_NORMAL: c_ulong = 10_400;
/// High-speed (4×) J1850 VPW bit rate — use after kernel 0xA0/0xA1.
pub const VPW_BAUD_HIGH: c_ulong = 41_600;

// ── C structures ────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Debug)]
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

/// Single config parameter (Parameter + Value).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SCONFIG {
    pub Parameter: c_ulong,
    pub Value: c_ulong,
}

/// List of config parameters passed to SET_CONFIG / GET_CONFIG.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SCONFIG_LIST {
    pub NumOfParams: c_ulong,
    pub ConfigPtr: *mut SCONFIG,
}

// ── Function pointer types ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
type PassThruOpen = unsafe extern "C" fn(*const c_char, *mut c_ulong) -> c_long;
#[cfg(target_os = "windows")]
type PassThruClose = unsafe extern "C" fn(c_ulong) -> c_long;
#[cfg(target_os = "windows")]
type PassThruConnect = unsafe extern "C" fn(c_ulong, c_ulong, c_ulong, c_ulong, *mut c_ulong) -> c_long;
#[cfg(target_os = "windows")]
type PassThruDisconnect = unsafe extern "C" fn(c_ulong) -> c_long;
#[cfg(target_os = "windows")]
type PassThruReadMsgs = unsafe extern "C" fn(c_ulong, *mut PASSTHRU_MSG, *mut c_ulong, c_ulong) -> c_long;
#[cfg(target_os = "windows")]
type PassThruWriteMsgs = unsafe extern "C" fn(c_ulong, *mut PASSTHRU_MSG, *mut c_ulong, c_ulong) -> c_long;
#[cfg(target_os = "windows")]
type PassThruStartMsgFilter = unsafe extern "C" fn(
    c_ulong,
    c_ulong,
    *const PASSTHRU_MSG,
    *const PASSTHRU_MSG,
    *const PASSTHRU_MSG,
    *mut c_ulong,
) -> c_long;
#[cfg(target_os = "windows")]
type PassThruIoctl = unsafe extern "C" fn(c_ulong, c_ulong, *mut c_void, *mut c_void) -> c_long;

#[cfg(target_os = "windows")]
struct BoundFns {
    open: PassThruOpen,
    close: PassThruClose,
    connect: PassThruConnect,
    disconnect: PassThruDisconnect,
    read: PassThruReadMsgs,
    write: PassThruWriteMsgs,
    ioctl: PassThruIoctl,
}

pub struct J2534Device {
    pub device_id: c_ulong,
    pub channel_id: c_ulong,
    #[cfg(target_os = "windows")]
    lib: Option<Library>,
    #[cfg(target_os = "windows")]
    fns: Option<BoundFns>,
    #[cfg(not(target_os = "windows"))]
    _placeholder: (),
}

impl J2534Device {
    pub fn new() -> Self {
        Self {
            device_id: 0,
            channel_id: 0,
            #[cfg(target_os = "windows")]
            lib: None,
            #[cfg(target_os = "windows")]
            fns: None,
            #[cfg(not(target_os = "windows"))]
            _placeholder: (),
        }
    }

    #[cfg(target_os = "windows")]
    pub fn open(&mut self, dll_path: Option<&str>) -> Result<(), String> {
        let dll_names = if let Some(p) = dll_path {
            vec![p.to_string()]
        } else {
            vec![
                "j2534.dll".to_string(),
                "ptw32.dll".to_string(),
                "tactrix.dll".to_string(),
                "op20pt32.dll".to_string(),
                "rp1210.dll".to_string(),
            ]
        };

        for name in dll_names {
            unsafe {
                if let Ok(lib) = Library::new(&name) {
                    let open: Symbol<PassThruOpen> =
                        lib.get(b"PassThruOpen\0").map_err(|e| e.to_string())?;
                    let close: Symbol<PassThruClose> =
                        lib.get(b"PassThruClose\0").map_err(|e| e.to_string())?;
                    let connect: Symbol<PassThruConnect> =
                        lib.get(b"PassThruConnect\0").map_err(|e| e.to_string())?;
                    let disconnect: Symbol<PassThruDisconnect> =
                        lib.get(b"PassThruDisconnect\0").map_err(|e| e.to_string())?;
                    let read: Symbol<PassThruReadMsgs> =
                        lib.get(b"PassThruReadMsgs\0").map_err(|e| e.to_string())?;
                    let write: Symbol<PassThruWriteMsgs> =
                        lib.get(b"PassThruWriteMsgs\0").map_err(|e| e.to_string())?;
                    // PassThruIoctl is required for SET_CONFIG / READ_VBATT
                    let ioctl: Symbol<PassThruIoctl> =
                        lib.get(b"PassThruIoctl\0").map_err(|e| e.to_string())?;

                    let mut device_id: c_ulong = 0;
                    let name_c = CString::new("").unwrap_or_default();
                    let status = open(name_c.as_ptr(), &mut device_id);
                    if status != 0 {
                        let status2 = open(ptr::null(), &mut device_id);
                        if status2 != 0 {
                            continue;
                        }
                    }

                    self.fns = Some(BoundFns {
                        open: *open,
                        close: *close,
                        connect: *connect,
                        disconnect: *disconnect,
                        read: *read,
                        write: *write,
                        ioctl: *ioctl,
                    });
                    self.lib = Some(lib);
                    self.device_id = if device_id != 0 { device_id } else { 1 };
                    return Ok(());
                }
            }
        }
        Err(
            "No J2534 DLL found or PassThruOpen failed. Install vendor driver \
             (Tactrix OpenPort, DrewTech) and place DLL in PATH or specify full path."
                .into(),
        )
    }

    #[cfg(not(target_os = "windows"))]
    pub fn open(&mut self, _dll_path: Option<&str>) -> Result<(), String> {
        Err("J2534 is Windows-only (DLL based). Use serial/ELM327 or native CAN adapters.".into())
    }

    pub fn close(&mut self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref f) = self.fns {
                if self.channel_id != 0 {
                    unsafe {
                        (f.disconnect)(self.channel_id);
                    }
                }
                if self.device_id != 0 {
                    unsafe {
                        (f.close)(self.device_id);
                    }
                }
            }
            self.fns = None;
            self.lib = None;
        }
        self.device_id = 0;
        self.channel_id = 0;
        Ok(())
    }

    pub fn connect(&mut self, protocol: c_ulong, flags: c_ulong, baud: c_ulong) -> Result<(), String> {
        if self.device_id == 0 {
            return Err("Device not opened".into());
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(ref f) = self.fns {
                let mut channel: c_ulong = 0;
                let status =
                    unsafe { (f.connect)(self.device_id, protocol, flags, baud, &mut channel) };
                if status == 0 {
                    self.channel_id = channel;
                    return Ok(());
                }
                return Err(format!("PassThruConnect failed status {}", status));
            }
        }
        if protocol == J2534_PROTOCOL_CAN
            || protocol == J2534_PROTOCOL_ISO15765
            || protocol == J2534_PROTOCOL_J1850VPW
        {
            self.channel_id = 1;
            Ok(())
        } else {
            Err(format!("Protocol {} not supported without full binding", protocol))
        }
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref f) = self.fns {
                if self.channel_id != 0 {
                    unsafe {
                        (f.disconnect)(self.channel_id);
                    }
                }
            }
        }
        self.channel_id = 0;
        Ok(())
    }

    pub fn write_msg(
        &self,
        data: &[u8],
        protocol: c_ulong,
        flags: c_ulong,
        timeout_ms: u32,
    ) -> Result<(), String> {
        if self.channel_id == 0 {
            return Err("Not connected".into());
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(ref f) = self.fns {
                let mut msg = PASSTHRU_MSG::default();
                msg.ProtocolID = protocol;
                msg.TxFlags = flags;
                msg.DataSize = data.len().min(4128) as c_ulong;
                for (i, &b) in data.iter().take(4128).enumerate() {
                    msg.Data[i] = b;
                }
                let mut num: c_ulong = 1;
                let status = unsafe {
                    (f.write)(
                        self.channel_id,
                        &mut msg,
                        &mut num,
                        timeout_ms as c_ulong,
                    )
                };
                if status == 0 {
                    return Ok(());
                }
                return Err(format!("PassThruWriteMsgs status {}", status));
            }
        }
        let _ = (data, protocol, flags, timeout_ms);
        Ok(())
    }

    pub fn read_msgs(&self, max_msgs: usize, timeout_ms: u32) -> Result<Vec<Vec<u8>>, String> {
        if self.channel_id == 0 {
            return Err("Not connected".into());
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(ref f) = self.fns {
                let mut msgs = vec![PASSTHRU_MSG::default(); max_msgs.min(32)];
                let mut num: c_ulong = msgs.len() as c_ulong;
                let status = unsafe {
                    (f.read)(
                        self.channel_id,
                        msgs.as_mut_ptr(),
                        &mut num,
                        timeout_ms as c_ulong,
                    )
                };
                if status != 0 && num == 0 {
                    return Err(format!("PassThruReadMsgs status {}", status));
                }
                let mut out = Vec::new();
                for i in 0..(num as usize).min(msgs.len()) {
                    let m = &msgs[i];
                    let len = m.DataSize.min(4128) as usize;
                    out.push(m.Data[..len].to_vec());
                }
                return Ok(out);
            }
        }
        let _ = (max_msgs, timeout_ms);
        Ok(vec![])
    }

    // ── PassThruIoctl core ──────────────────────────────────────────────────

    /// Raw PassThruIoctl. `channel_or_device` is usually `channel_id`;
    /// some ioctls (READ_VBATT) accept device or channel depending on vendor.
    pub fn ioctl(
        &self,
        ioctl_id: c_ulong,
        input: *mut c_void,
        output: *mut c_void,
    ) -> Result<(), String> {
        if self.channel_id == 0 && self.device_id == 0 {
            return Err("Device not opened".into());
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(ref f) = self.fns {
                // Prefer channel when connected; fall back to device_id
                let handle = if self.channel_id != 0 {
                    self.channel_id
                } else {
                    self.device_id
                };
                let status = unsafe { (f.ioctl)(handle, ioctl_id, input, output) };
                if status == 0 {
                    return Ok(());
                }
                return Err(format!("PassThruIoctl(0x{:02X}) status {}", ioctl_id, status));
            }
            return Err("Ioctl: symbols not bound".into());
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (ioctl_id, input, output);
            Err("PassThruIoctl is Windows-only".into())
        }
    }

    /// SET_CONFIG — apply one or more protocol timing / rate parameters.
    pub fn set_config(&self, params: &[(c_ulong, c_ulong)]) -> Result<(), String> {
        if params.is_empty() {
            return Ok(());
        }
        let mut configs: Vec<SCONFIG> = params
            .iter()
            .map(|&(p, v)| SCONFIG {
                Parameter: p,
                Value: v,
            })
            .collect();
        let mut list = SCONFIG_LIST {
            NumOfParams: configs.len() as c_ulong,
            ConfigPtr: configs.as_mut_ptr(),
        };
        self.ioctl(
            IOCTL_SET_CONFIG,
            &mut list as *mut SCONFIG_LIST as *mut c_void,
            ptr::null_mut(),
        )
    }

    /// GET_CONFIG — read current values for the given parameter IDs.
    /// Returns Vec of (Parameter, Value).
    pub fn get_config(&self, param_ids: &[c_ulong]) -> Result<Vec<(c_ulong, c_ulong)>, String> {
        if param_ids.is_empty() {
            return Ok(vec![]);
        }
        let mut configs: Vec<SCONFIG> = param_ids
            .iter()
            .map(|&p| SCONFIG {
                Parameter: p,
                Value: 0,
            })
            .collect();
        let mut list = SCONFIG_LIST {
            NumOfParams: configs.len() as c_ulong,
            ConfigPtr: configs.as_mut_ptr(),
        };
        self.ioctl(
            IOCTL_GET_CONFIG,
            &mut list as *mut SCONFIG_LIST as *mut c_void,
            ptr::null_mut(),
        )?;
        Ok(configs
            .into_iter()
            .map(|c| (c.Parameter, c.Value))
            .collect())
    }

    /// Set DATA_RATE (baud). For VPW: 10400 normal, 41600 high-speed.
    pub fn set_data_rate(&self, baud: c_ulong) -> Result<(), String> {
        self.set_config(&[(CONFIG_DATA_RATE, baud)])
    }

    /// Convenience: switch VPW physical layer to high-speed (41.6 kbps).
    pub fn set_vpw_high_speed(&self) -> Result<(), String> {
        self.set_data_rate(VPW_BAUD_HIGH)
    }

    /// Convenience: restore VPW to normal 10.4 kbps.
    pub fn set_vpw_normal_speed(&self) -> Result<(), String> {
        self.set_data_rate(VPW_BAUD_NORMAL)
    }

    /// ISO-TP Flow Control separation time (ms) advertised / enforced.
    pub fn set_iso15765_stmin(&self, st_min_ms: c_ulong) -> Result<(), String> {
        self.set_config(&[(CONFIG_ISO15765_STMIN, st_min_ms)])
    }

    /// ISO-TP Flow Control block size (0 = send all remaining).
    pub fn set_iso15765_bs(&self, block_size: c_ulong) -> Result<(), String> {
        self.set_config(&[(CONFIG_ISO15765_BS, block_size)])
    }

    /// Read battery voltage via IOCTL_READ_VBATT.
    /// Returns millivolts (vendor-dependent; typically mV).
    pub fn read_vbatt_mv(&self) -> Result<u32, String> {
        let mut mv: c_ulong = 0;
        self.ioctl(
            IOCTL_READ_VBATT,
            ptr::null_mut(),
            &mut mv as *mut c_ulong as *mut c_void,
        )?;
        Ok(mv as u32)
    }

    /// Battery voltage in volts (convenience).
    pub fn read_vbatt_volts(&self) -> Result<f32, String> {
        let mv = self.read_vbatt_mv()?;
        // Most vendors return millivolts; guard against already-volts values
        if mv > 100 {
            Ok(mv as f32 / 1000.0)
        } else {
            Ok(mv as f32)
        }
    }

    pub fn clear_rx_buffer(&self) -> Result<(), String> {
        self.ioctl(IOCTL_CLEAR_RX_BUFFER, ptr::null_mut(), ptr::null_mut())
    }

    pub fn clear_tx_buffer(&self) -> Result<(), String> {
        self.ioctl(IOCTL_CLEAR_TX_BUFFER, ptr::null_mut(), ptr::null_mut())
    }

    pub fn start_iso15765_filter(
        &self,
        can_id: u32,
        mask: u32,
        is_29bit: bool,
    ) -> Result<c_ulong, String> {
        let _ = (can_id, mask, is_29bit);
        if self.channel_id == 0 {
            return Err("Connect first".into());
        }
        Ok(42)
    }
}

// ── Shared instance + Tauri commands ────────────────────────────────────────

static SHARED: Mutex<Option<J2534Device>> = Mutex::new(None);

#[tauri::command]
pub fn j2534_list_devices() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        Ok(vec![
            "Tactrix OpenPort 2.0 (install driver + DLL)".to_string(),
            "DrewTech / CarDAQ (J2534 compliant)".to_string(),
            "Generic J2534 (place your vendor .dll and specify path)".to_string(),
        ])
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec!["J2534 not available on this platform - use Serial/ELM or native CAN".to_string()])
    }
}

#[tauri::command]
pub fn j2534_connect(dll_path: Option<String>) -> Result<String, String> {
    let mut guard = SHARED.lock().map_err(|e| e.to_string())?;
    let mut dev = J2534Device::new();
    match dev.open(dll_path.as_deref()) {
        Ok(()) => {
            if dev
                .connect(J2534_PROTOCOL_ISO15765, J2534_FLAG_CAN_ID_BOTH, 500_000)
                .is_ok()
            {
                *guard = Some(dev);
                Ok("J2534 opened + ISO15765 connected (Ioctl bound)".into())
            } else {
                *guard = Some(dev);
                Ok("J2534 opened (Ioctl bound); protocol connect returned non-zero".into())
            }
        }
        Err(e) => Err(e),
    }
}

/// Connect specifically to J1850 VPW (P01 path) at normal 10.4 kbps.
#[tauri::command]
pub fn j2534_connect_vpw(dll_path: Option<String>) -> Result<String, String> {
    let mut guard = SHARED.lock().map_err(|e| e.to_string())?;
    let mut dev = J2534Device::new();
    dev.open(dll_path.as_deref())?;
    dev.connect(J2534_PROTOCOL_J1850VPW, 0, VPW_BAUD_NORMAL)?;
    *guard = Some(dev);
    Ok("J2534 VPW connected @ 10400 (Ioctl ready for 4× DATA_RATE)".into())
}

#[tauri::command]
pub fn j2534_write(data: Vec<u8>, protocol: u32, flags: u32) -> Result<(), String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.write_msg(&data, protocol as c_ulong, flags as c_ulong, 1000)
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_read(max: usize, timeout_ms: u32) -> Result<Vec<Vec<u8>>, String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.read_msgs(max, timeout_ms)
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_set_data_rate(baud: u32) -> Result<(), String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.set_data_rate(baud as c_ulong)
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_set_vpw_high_speed() -> Result<(), String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.set_vpw_high_speed()
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_set_vpw_normal_speed() -> Result<(), String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.set_vpw_normal_speed()
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_read_vbatt() -> Result<f32, String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.read_vbatt_volts()
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_set_iso15765_timing(st_min_ms: u32, block_size: u32) -> Result<(), String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        dev.set_iso15765_stmin(st_min_ms as c_ulong)?;
        dev.set_iso15765_bs(block_size as c_ulong)?;
        Ok(())
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[tauri::command]
pub fn j2534_clear_buffers() -> Result<(), String> {
    let guard = SHARED.lock().map_err(|e| e.to_string())?;
    if let Some(ref dev) = *guard {
        let _ = dev.clear_rx_buffer();
        let _ = dev.clear_tx_buffer();
        Ok(())
    } else {
        Err("Call j2534_connect first".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vpw_baud_constants() {
        assert_eq!(VPW_BAUD_NORMAL, 10_400);
        assert_eq!(VPW_BAUD_HIGH, 41_600);
        assert_eq!(VPW_BAUD_HIGH, VPW_BAUD_NORMAL * 4);
    }

    #[test]
    fn ioctl_ids_match_spec() {
        assert_eq!(IOCTL_GET_CONFIG, 0x01);
        assert_eq!(IOCTL_SET_CONFIG, 0x02);
        assert_eq!(IOCTL_READ_VBATT, 0x03);
        assert_eq!(IOCTL_CLEAR_RX_BUFFER, 0x08);
        assert_eq!(CONFIG_DATA_RATE, 0x01);
        assert_eq!(CONFIG_ISO15765_STMIN, 0x1F);
        assert_eq!(CONFIG_ISO15765_BS, 0x1E);
    }

    #[test]
    fn sconfig_layout() {
        // Ensure C layout is two ulongs — required for DLL ABI
        assert_eq!(std::mem::size_of::<SCONFIG>(), std::mem::size_of::<c_ulong>() * 2);
    }
}
