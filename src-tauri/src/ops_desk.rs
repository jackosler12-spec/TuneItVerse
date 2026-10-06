//! Operational desk: flash preflight, string harvest, offline DTC lookup.
//! Nothing here flashes an ECU or flips write_allowed.

use serde_json::{json, Value};

const MAX_STRINGS: usize = 200;

pub fn harvest_strings(data: &[u8], min_len: Option<usize>) -> Value {
    let min_len = min_len.unwrap_or(5).clamp(4, 32);
    let mut strings = Vec::new();
    let mut i = 0usize;
    let mut truncated = false;
    while i < data.len() {
        if (0x20..0x7f).contains(&data[i]) {
            let start = i;
            while i < data.len() && (0x20..0x7f).contains(&data[i]) {
                i += 1;
            }
            if i - start >= min_len {
                if strings.len() >= MAX_STRINGS {
                    truncated = true;
                    break;
                }
                strings.push(json!({
                    "offset": format!("0x{:06X}", start),
                    "text": String::from_utf8_lossy(&data[start..i]).to_string()
                }));
            }
        } else {
            i += 1;
        }
    }
    json!({
        "count": strings.len(),
        "min_len": min_len,
        "truncated": truncated,
        "strings": strings,
        "write_allowed": false,
        "note": "Printable runs only. Not an OS identification and not a write enable."
    })
}

fn family_name(ident: &Value) -> String {
    ident.get("family")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("unresolved")
        .to_string()
}

/// Go / no-go checklist for the loaded image. Ready never means the ECU was written.
pub fn flash_preflight(data: &[u8], voltage_v: Option<f64>, min_voltage_v: Option<f64>) -> Value {
    let mut blockers: Vec<String> = Vec::new();
    let warnings = vec![
        "Take a verified backup before any write. Preflight does not read the ECU.".to_string(),
        "Personal dump only. Do not flash a file you cannot restore.".to_string(),
    ];
    if data.is_empty() {
        blockers.push("No image loaded.".into());
        return json!({
            "ready": false,
            "write_allowed": false,
            "family": "unresolved",
            "bytes": 0,
            "blockers": blockers,
            "warnings": warnings,
            "note": "Preflight does not flash and cannot enable write."
        });
    }

    let ident = crate::v29_tools::identify_bin(data);
    let family = family_name(&ident);
    let write_allowed = ident.get("write_allowed").and_then(|v| v.as_bool()).unwrap_or(false);
    if family == "unresolved" {
        blockers.push("Family unresolved. Identify the OS before any write.".into());
    }
    if !write_allowed {
        blockers.push(format!("Write path is not live for {family}. Catalog-only families stay blocked."));
    }
    if ident.get("size_collision").and_then(|v| v.as_bool()).unwrap_or(false) {
        blockers.push("Size collision. Confirm the OS string before treating this as a known family.".into());
    }
    let os_confirmed = ident.get("gm_p01_os").and_then(|v| v.as_bool()).unwrap_or(false)
        || ident.get("family_by_os").and_then(|v| v.as_str()).is_some();
    if write_allowed && !os_confirmed {
        blockers.push("Write family matched by size only. Confirm an OS/part string before preflight can go ready.".into());
    }

    let checksum = match crate::checksum::validate_checksums(data) {
        Ok(cs) => {
            if !cs.all_valid {
                blockers.push(format!(
                    "Checksum not all valid (failed {}, method {}).",
                    cs.failed_count, cs.method_used
                ));
            }
            json!({
                "ecu_family": cs.ecu_family,
                "method": cs.method_used,
                "all_valid": cs.all_valid,
                "failed": cs.failed_count
            })
        }
        Err(e) => {
            blockers.push(format!("Checksum check failed: {e}"));
            json!({"error": e})
        }
    };

    let min_v = min_voltage_v.unwrap_or(12.5);
    let voltage_ok = match voltage_v {
        None => {
            blockers.push("Battery voltage not supplied. Read PID 0x42 or J2534 Vbatt before write.".into());
            false
        }
        Some(v) if !v.is_finite() || v <= 0.0 => {
            blockers.push("Voltage 0 is bench bypass only. Preflight stays not ready.".into());
            false
        }
        Some(v) if v < min_v => {
            blockers.push(format!("Voltage {v:.2} V is below the {min_v:.1} V gate."));
            false
        }
        Some(_) => true,
    };

    json!({
        "ready": blockers.is_empty() && write_allowed && voltage_ok,
        "write_allowed": write_allowed,
        "family": family,
        "bytes": data.len(),
        "os_confirmed": os_confirmed,
        "checksum": checksum,
        "voltage_v": voltage_v,
        "min_voltage_v": min_v,
        "voltage_ok": voltage_ok,
        "blockers": blockers,
        "warnings": warnings,
        "note": "Preflight does not flash and cannot enable write."
    })
}

fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

pub fn lookup_dtc(code: &str) -> Value {
    let normalized = normalize_code(code);
    if normalized.len() < 5 {
        return json!({
            "code": normalized,
            "known": false,
            "description": "Need a code like P0300.",
            "write_allowed": false
        });
    }
    let description = KNOWN.iter().find(|(k, _)| *k == normalized).map(|(_, d)| *d);
    let class = match normalized.chars().next() {
        Some('P') => "powertrain",
        Some('C') => "chassis",
        Some('B') => "body",
        Some('U') => "network",
        _ => "unknown",
    };
    let generic = normalized.chars().nth(1) == Some('0');
    json!({
        "code": normalized,
        "known": description.is_some(),
        "description": description.unwrap_or(if generic {
            "Generic SAE code. Not in the offline list — check the service manual for this ECU."
        } else {
            "Manufacturer-specific code. Offline list is generic SAE only."
        }),
        "class": class,
        "generic_sae": generic,
        "write_allowed": false,
        "note": "Lookup does not clear codes and does not enable write."
    })
}

const KNOWN: &[(&str, &str)] = &[
    ("P0100", "Mass air flow circuit"),
    ("P0101", "Mass air flow range/performance"),
    ("P0106", "Manifold pressure range/performance"),
    ("P0113", "Intake air temperature circuit high"),
    ("P0117", "Engine coolant temperature circuit low"),
    ("P0118", "Engine coolant temperature circuit high"),
    ("P0120", "Throttle position circuit"),
    ("P0128", "Coolant thermostat below regulating temperature"),
    ("P0131", "O2 sensor circuit low, bank 1 sensor 1"),
    ("P0134", "O2 sensor circuit no activity, bank 1 sensor 1"),
    ("P0171", "System too lean, bank 1"),
    ("P0172", "System too rich, bank 1"),
    ("P0174", "System too lean, bank 2"),
    ("P0175", "System too rich, bank 2"),
    ("P0300", "Random/multiple cylinder misfire"),
    ("P0301", "Cylinder 1 misfire"),
    ("P0302", "Cylinder 2 misfire"),
    ("P0303", "Cylinder 3 misfire"),
    ("P0304", "Cylinder 4 misfire"),
    ("P0305", "Cylinder 5 misfire"),
    ("P0306", "Cylinder 6 misfire"),
    ("P0307", "Cylinder 7 misfire"),
    ("P0308", "Cylinder 8 misfire"),
    ("P0325", "Knock sensor circuit, bank 1"),
    ("P0335", "Crankshaft position sensor circuit"),
    ("P0340", "Camshaft position sensor circuit"),
    ("P0420", "Catalyst system efficiency below threshold, bank 1"),
    ("P0430", "Catalyst system efficiency below threshold, bank 2"),
    ("P0440", "Evaporative emission system"),
    ("P0442", "Evaporative emission system leak detected, small"),
    ("P0455", "Evaporative emission system leak detected, large"),
    ("P0500", "Vehicle speed sensor"),
    ("P0505", "Idle air control system"),
    ("P0507", "Idle air control RPM higher than expected"),
    ("P0601", "Internal control module memory check sum"),
    ("P0700", "Transmission control system"),
    ("U0001", "High speed CAN communication bus"),
    ("U0100", "Lost communication with ECM/PCM"),
];

pub fn self_check_ok() -> bool {
    let blocked = flash_preflight(&[1, 2, 3, 4], Some(11.0), None);
    let strings = harvest_strings(b"\0OSID12225074\0", Some(5));
    let dtc = lookup_dtc("p0300");
    blocked["ready"] == false
        && blocked["write_allowed"] == false
        && strings["write_allowed"] == false
        && strings["count"].as_u64().unwrap_or(0) >= 1
        && dtc["known"] == true
        && dtc["write_allowed"] == false
}

#[tauri::command]
pub fn flash_preflight_cmd(data: Vec<u8>, voltage_v: Option<f64>, min_voltage_v: Option<f64>) -> Result<String, String> {
    Ok(flash_preflight(&data, voltage_v, min_voltage_v).to_string())
}

#[tauri::command]
pub fn bin_strings_cmd(data: Vec<u8>, min_len: Option<u32>) -> Result<String, String> {
    Ok(harvest_strings(&data, min_len.map(|n| n as usize)).to_string())
}

#[tauri::command]
pub fn dtc_lookup_cmd(code: String) -> Result<String, String> {
    Ok(lookup_dtc(&code).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_image_is_not_ready_and_cannot_enable_write() {
        let v = flash_preflight(&[9, 8, 7], Some(13.2), None);
        assert_eq!(v["ready"], false);
        assert_eq!(v["write_allowed"], false);
        assert!(v["blockers"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn low_voltage_blocks_even_if_other_checks_fail() {
        let v = flash_preflight(&[1, 2, 3, 4], Some(10.5), Some(12.5));
        let blockers = v["blockers"].as_array().unwrap();
        assert!(blockers.iter().any(|b| b.as_str().unwrap_or("").contains("10.50")));
        assert_eq!(v["ready"], false);
    }

    #[test]
    fn strings_skip_short_runs_and_keep_write_off() {
        let v = harvest_strings(b"ab\0HELLO\xff", Some(5));
        assert_eq!(v["count"], 1);
        assert_eq!(v["strings"][0]["text"], "HELLO");
        assert_eq!(v["write_allowed"], false);
    }

    #[test]
    fn dtc_lookup_is_generic_and_offline() {
        let known = lookup_dtc(" P0301 ");
        assert_eq!(known["code"], "P0301");
        assert_eq!(known["known"], true);
        assert_eq!(known["write_allowed"], false);
        let unknown = lookup_dtc("P1999");
        assert_eq!(unknown["known"], false);
        assert_eq!(unknown["generic_sae"], false);
    }
}
