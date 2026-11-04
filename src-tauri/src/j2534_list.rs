//! J2534 device discovery.
//!
//! On Windows, J2534 v04.04 drivers register under
//! `HKLM\SOFTWARE\PassThruSupport.04.04\<Device>` (32-bit drivers land under
//! `WOW6432Node`). Each device key carries `Name`, `Vendor`, `FunctionLibrary`
//! (DLL path) and DWORD protocol flags such as `CAN`, `ISO15765`, `J1850VPW`.
//! We query those keys with the stock `reg.exe` (no extra crate, so Linux CI
//! and Cargo.lock stay untouched) and parse the output here.
//!
//! The DLL bitness is read from its PE header so the UI can tell the user
//! honestly when a 32-bit driver cannot be loaded by a 64-bit build.
//! Non-Windows returns an empty list plus a note: J2534 is a Win32 API.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct J2534Entry {
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub function_library: String,
    /// Protocol flags whose registry DWORD is non-zero (e.g. "CAN", "J1850VPW").
    pub protocols: Vec<String>,
    /// True when the key came from WOW6432Node (32-bit driver registration).
    pub wow64: bool,
    /// "x86", "x64", "arm64", "unknown" or "missing" (file not found).
    pub dll_arch: String,
    /// Whether this build can load the DLL (arch matches the running process).
    pub loadable: bool,
    pub note: String,
}

const PROTOCOL_KEYS: &[&str] = &[
    "CAN", "ISO15765", "J1850VPW", "J1850PWM", "ISO9141", "ISO14230", "SCI_A_ENGINE",
    "SCI_A_TRANS", "SCI_B_ENGINE", "SCI_B_TRANS", "SW_CAN_PS", "GM_UART_PS",
];

fn parse_dword(v: &str) -> Option<u32> {
    let t = v.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(h, 16).ok()
    } else {
        t.parse().ok()
    }
}

/// Parse `reg query <root> /s` output into structured entries.
/// Only keys that have at least a Name or FunctionLibrary are returned.
pub fn parse_reg_query_entries(stdout: &str) -> Vec<J2534Entry> {
    let mut out = Vec::new();
    let mut cur: Option<J2534Entry> = None;
    let push = |cur: &mut Option<J2534Entry>, out: &mut Vec<J2534Entry>| {
        if let Some(e) = cur.take() {
            if !e.name.is_empty() || !e.function_library.is_empty() {
                out.push(e);
            }
        }
    };
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("HKEY_") {
            push(&mut cur, &mut out);
            let wow64 = line.to_ascii_uppercase().contains("WOW6432NODE");
            cur = Some(J2534Entry { key: line.to_string(), wow64, ..Default::default() });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        // reg.exe separates columns with 4 spaces; names have no spaces in practice.
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 || !parts[1].starts_with("REG_") {
            continue;
        }
        let key = parts[0].to_ascii_uppercase();
        let val = parts[2..].join(" ");
        match key.as_str() {
            "NAME" => e.name = val,
            "VENDOR" => e.vendor = val,
            "FUNCTIONLIBRARY" | "FUNCTION_LIBRARY" => e.function_library = val,
            k if parts[1] == "REG_DWORD" && PROTOCOL_KEYS.contains(&k) => {
                if parse_dword(&val).unwrap_or(0) != 0 {
                    e.protocols.push(k.to_string());
                }
            }
            _ => {}
        }
    }
    push(&mut cur, &mut out);
    out
}

/// Back-compat flat rows ("name | vendor | dll").
pub fn parse_reg_query(stdout: &str) -> Vec<String> {
    parse_reg_query_entries(stdout).iter().map(row_string).collect()
}

/// Read the PE machine type from DLL bytes. Returns None if not a PE image.
pub fn pe_machine(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return None;
    }
    let off = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    if bytes.len() < off + 6 || &bytes[off..off + 4] != b"PE\0\0" {
        return None;
    }
    let m = u16::from_le_bytes([bytes[off + 4], bytes[off + 5]]);
    Some(match m {
        0x014C => "x86",
        0x8664 => "x64",
        0xAA64 => "arm64",
        _ => "unknown",
    })
}

fn process_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "unknown"
    }
}

/// Fill dll_arch / loadable / note from the file on disk.
pub fn annotate(e: &mut J2534Entry) {
    let arch = if e.function_library.is_empty() {
        "missing"
    } else {
        match std::fs::read(&e.function_library) {
            Ok(b) => pe_machine(&b).unwrap_or("unknown"),
            Err(_) => "missing",
        }
    };
    e.dll_arch = arch.to_string();
    let me = process_arch();
    e.loadable = arch == me;
    e.note = match arch {
        "missing" => "FunctionLibrary DLL not found on disk; reinstall the vendor driver.".into(),
        "unknown" => "DLL is not a recognised PE image; not loadable.".into(),
        a if a == me => "DLL arch matches this build.".into(),
        a => format!(
            "{} DLL cannot be loaded by this {} build (most J2534 04.04 drivers are 32-bit). \
             Use a {} build of the app or a vendor 64-bit driver.",
            a, me, a
        ),
    };
}

fn row_string(e: &J2534Entry) -> String {
    let mut s = format!(
        "{} | {} | {}",
        if e.name.is_empty() { "J2534 device" } else { &e.name },
        if e.vendor.is_empty() { "?" } else { &e.vendor },
        if e.function_library.is_empty() { "dll path unknown" } else { &e.function_library }
    );
    if !e.protocols.is_empty() {
        s.push_str(&format!(" | {}", e.protocols.join(",")));
    }
    if !e.dll_arch.is_empty() {
        s.push_str(&format!(" | {}{}", e.dll_arch, if e.loadable { "" } else { " (not loadable)" }));
    }
    s
}

#[cfg(target_os = "windows")]
fn query_registry() -> Vec<J2534Entry> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let roots = [
        r"HKLM\SOFTWARE\PassThruSupport.04.04",
        r"HKLM\SOFTWARE\WOW6432Node\PassThruSupport.04.04",
    ];
    let mut found: Vec<J2534Entry> = Vec::new();
    for root in roots {
        // /reg:64 so a 32-bit build still sees the native view; WOW6432Node is explicit.
        let out = std::process::Command::new("reg")
            .args(["query", root, "/s", "/reg:64"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                for mut e in parse_reg_query_entries(&String::from_utf8_lossy(&o.stdout)) {
                    if found.iter().any(|f| f.function_library.eq_ignore_ascii_case(&e.function_library) && f.name == e.name) {
                        continue;
                    }
                    annotate(&mut e);
                    found.push(e);
                }
            }
        }
    }
    found
}

/// Registered devices (empty on non-Windows).
pub fn discover() -> Vec<J2534Entry> {
    #[cfg(target_os = "windows")]
    {
        query_registry()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

/// DLL paths to try when the user gave none: loadable registry entries first.
pub fn registry_dll_candidates() -> Vec<String> {
    let mut v: Vec<J2534Entry> = discover();
    v.sort_by_key(|e| !e.loadable);
    v.into_iter().filter(|e| e.loadable).map(|e| e.function_library).collect()
}

#[tauri::command(async)]
pub fn j2534_list_devices() -> Result<Vec<String>, String> {
    let list = discover();
    if !list.is_empty() {
        return Ok(list.iter().map(row_string).collect());
    }
    if cfg!(target_os = "windows") {
        Ok(vec!["No PassThruSupport.04.04 registry keys. Install a vendor J2534 driver, then pick the .dll path.".into()])
    } else {
        Ok(vec!["J2534 PassThru is a Windows API. Use Serial/ELM on this OS. On Windows this command walks HKLM\\SOFTWARE\\PassThruSupport.04.04.".to_string()])
    }
}

#[tauri::command(async)]
pub fn j2534_list_devices_detailed() -> Result<String, String> {
    let list = discover();
    serde_json::to_string(&serde_json::json!({
        "platform_supported": cfg!(target_os = "windows"),
        "process_arch": process_arch(),
        "devices": list,
        "verified_on_hardware": false,
    }))
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\PassThruSupport.04.04\\OpenPort 2.0\n    Name    REG_SZ    Tactrix OpenPort 2.0\n    Vendor    REG_SZ    Tactrix\n    FunctionLibrary    REG_SZ    C:\\Windows\\SysWOW64\\op20pt32.dll\n    CAN    REG_DWORD    0x1\n    ISO15765    REG_DWORD    0x1\n    J1850VPW    REG_DWORD    0x1\n    J1850PWM    REG_DWORD    0x0\n\nHKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\PassThruSupport.04.04\\OpenPort 2.0\\Parameter\n    ConfigApplication    REG_SZ    C:\\x.exe\n";

    #[test]
    fn parses_reg_query_block() {
        let rows = parse_reg_query(SAMPLE);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains("Tactrix OpenPort 2.0"));
        assert!(rows[0].contains("op20pt32.dll"));
    }

    #[test]
    fn parses_protocol_flags_and_wow64() {
        let e = &parse_reg_query_entries(SAMPLE)[0];
        assert!(e.wow64);
        assert_eq!(e.protocols, vec!["CAN", "ISO15765", "J1850VPW"]);
        assert_eq!(e.vendor, "Tactrix");
    }

    #[test]
    fn subkeys_without_name_or_library_are_skipped() {
        assert_eq!(parse_reg_query_entries(SAMPLE).len(), 1);
        assert!(parse_reg_query_entries("garbage\n  Name REG_SZ x\n").is_empty());
    }

    #[test]
    fn pe_machine_detection() {
        let mut b = vec![0u8; 0x100];
        b[0] = b'M'; b[1] = b'Z';
        b[0x3C] = 0x80;
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&0x014Cu16.to_le_bytes());
        assert_eq!(pe_machine(&b), Some("x86"));
        b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        assert_eq!(pe_machine(&b), Some("x64"));
        assert_eq!(pe_machine(b"not a pe"), None);
    }

    #[test]
    fn missing_dll_is_not_loadable() {
        let mut e = J2534Entry { function_library: "/nonexistent/x.dll".into(), ..Default::default() };
        annotate(&mut e);
        assert_eq!(e.dll_arch, "missing");
        assert!(!e.loadable);
    }

    #[test]
    fn non_windows_discovers_nothing() {
        if !cfg!(target_os = "windows") {
            assert!(discover().is_empty());
            assert!(registry_dll_candidates().is_empty());
        }
    }
}
