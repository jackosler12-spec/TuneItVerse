#[tauri::command]
fn list_supported_protocols() -> Result<Vec<String>, String> {
    Ok(vec!["auto".into(), "vpw".into(), "can".into(), "kwp".into(), "consult".into(), "uds".into()])
}
#[tauri::command]
fn list_supported_ecus() -> Result<Vec<String>, String> { Ok(ecu_database::list_supported_ecu_families()) }
#[tauri::command]
fn list_ecu_catalog() -> Result<String, String> {
    let rows: Vec<serde_json::Value> = ecu_database::load_ecu_database()
        .into_iter()
        .map(|e| json!({
            "ecu_family": e.ecu_family,
            "display_name": e.display_name,
            "protocol": e.protocol,
            "bin_size_bytes": e.bin_size_bytes,
            "hardware": e.hardware,
            "vehicles": e.vehicles,
            "checksum": e.checksum.r#type,
            "security": e.security_access.r#type,
            "write_allowed": matches!(e.ecu_family.as_str(), "P01_0411" | "EDC16C41"),
            "status": if matches!(e.ecu_family.as_str(), "P01_0411" | "EDC16C41") { "write path live" } else { "identify/report — write blocked until measured corrector" },
        }))
        .collect();
    Ok(json!(rows).to_string())
}
#[tauri::command]
fn get_ecu_info(family_or_os: String) -> Result<String, String> {
    if let Some(e) = ecu_database::get_ecu_by_os_id(&family_or_os).or_else(|| ecu_database::get_ecu_by_family(&family_or_os)) {
        Ok(serde_json::to_string_pretty(&e).unwrap_or_else(|_| "{}".into()))
    } else {
        Ok(json!({"ecu_family": family_or_os, "display_name": "Unknown / not in DB"}).to_string())
    }
}

fn pull_mode01(port: &mut Box<dyn SerialPort + Send>, pid: u8) -> Option<Vec<u8>> {
    crate::transport::pull_mode01(port, pid)
}

#[tauri::command]
fn read_properties() -> Result<String, String> {
    let protocol = STATE.lock().map(|g| g.protocol.clone()).unwrap_or_default();
    let inner = with_port(|port| {
        let mut vin = "UNREAD".to_string();
        let mut calid = "UNREAD".to_string();
        if let Some(parsed) = crate::transport::pull_mode09(port, 0x02) {
            if parsed.len() >= 8 { vin = parsed; }
        }
        if let Some(parsed) = crate::transport::pull_mode09(port, 0x04) {
            if !parsed.is_empty() { calid = parsed; }
        }
        let os_id = if calid != "UNREAD" { calid.clone() } else { "UNREAD".to_string() };
        let ecu = crate::ecu_database::get_ecu_by_os_id(&os_id);
        Ok((os_id, vin, calid, ecu))
    });
    match inner {
        Ok((os_id, vin, calid, ecu)) => {
            if let Ok(mut guard) = STATE.lock() {
                if os_id != "UNREAD" { guard.last_os_id = Some(os_id.clone()); }
                if let Some(e) = ecu.as_ref() { guard.last_family = Some(e.ecu_family.clone()); }
            }
            Ok(json!({
                "os_id": os_id,
                "vin": vin,
                "calid": calid,
                "hardware": ecu.as_ref().map(|e| e.hardware.clone()).unwrap_or_else(|| "UNREAD".into()),
                "ecu_type": ecu.as_ref().map(|e| e.ecu_family.clone()).unwrap_or_else(|| "UNREAD".into()),
                "protocol": protocol,
                "status": "live"
            }).to_string())
        }
        Err(_) => Ok(json!({"os_id":"UNREAD","vin":"UNREAD","calid":"UNREAD","hardware":"UNREAD","ecu_type":"UNREAD","protocol":"offline","status":"Offline"}).to_string())
    }
}

#[tauri::command]
fn read_ecu_data() -> Result<String, String> {
    with_port(|port| Ok(serde_json::Value::Object(crate::transport::decode_live_map(port)).to_string()))
        .or_else(|_| Ok(json!({"source":"offline","pids_decoded":0,"honest":true,"note":"Offline — no invented live PIDs."}).to_string()))
}

#[tauri::command] fn get_logging_templates() -> Result<String, String> { Ok(serde_json::to_string(&logging::list_templates()).unwrap_or_else(|_| "[]".into())) }
#[tauri::command] fn log_get_status() -> Result<String, String> { Ok(serde_json::to_string(&logging::get_status()).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_start(rate_hz: Option<f64>, session_name: Option<String>) -> Result<String, String> { Ok(serde_json::to_string(&logging::start_session(rate_hz, session_name)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_stop() -> Result<String, String> { Ok(serde_json::to_string(&logging::stop_session()?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_set_channels(enabled_ids: Vec<String>) -> Result<String, String> { Ok(serde_json::to_string(&logging::set_channels(enabled_ids)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_apply_template(template_id: String) -> Result<String, String> { Ok(serde_json::to_string(&logging::apply_template(&template_id)?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command]
fn log_capture_sample() -> Result<String, String> {
    use crate::pid_decode::*;
    let live_overrides = with_port(|port| {
        let mut map: HashMap<String, f64> = HashMap::new();
        if let Some(d)=pull_mode01(port,0x0C){ if let Some(v)=decode_engine_rpm(&d){ map.insert("rpm".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0B){ if let Some(v)=decode_map(&d){ map.insert("map".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x05){ if let Some(v)=decode_ect(&d){ map.insert("ect".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x11){ if let Some(v)=decode_throttle_pos(&d){ map.insert("tps".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0F){ if let Some(v)=decode_iat(&d){ map.insert("iat".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0E){ if let Some(v)=decode_timing_advance(&d){ map.insert("spark".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x06){ if let Some(v)=decode_stft_bank1(&d){ map.insert("stft".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x07){ if let Some(v)=decode_ltft_bank1(&d){ map.insert("ltft".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x10){ if let Some(v)=decode_maf_obd(&d){ map.insert("maf".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x0D){ if let Some(v)=decode_vss(&d){ map.insert("vss".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x04){ if let Some(v)=decode_engine_load(&d){ map.insert("load".into(), v as f64);} }
        if let Some(v)=crate::flash::read_battery_voltage(port){ map.insert("batt".into(), v as f64); }
        if let Some(d)=pull_mode01(port,0x14){ if let Some(v)=decode_o2_b1s1_obd(&d){ map.insert("o2b1s1".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x15){ if let Some(v)=decode_o2_b1s2_obd(&d){ map.insert("o2b1s2".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x33){ if let Some(&b)=d.first(){ map.insert("baro".into(), b as f64);} }
        if let Some(d)=pull_mode01(port,0x03){ if let Some(v)=decode_fuel_system_status(&d){ map.insert("fuel_status".into(), v as f64);} }
        if let Some(d)=pull_mode01(port,0x2F){ if let Some(v)=decode_fuel_level(&d){ map.insert("fuel_level".into(), v as f64);} }
        Ok(map)
    }).ok();
    Ok(serde_json::to_string(&logging::capture_sample(live_overrides)?).unwrap_or_else(|_| "{}".into()))
}
#[tauri::command] fn log_get_samples(limit: Option<usize>) -> Result<String, String> { Ok(serde_json::to_string(&logging::get_samples(limit)).unwrap_or_else(|_| "[]".into())) }
#[tauri::command] fn log_clear() -> Result<String, String> { Ok(serde_json::to_string(&logging::clear_samples()?).unwrap_or_else(|_| "{}".into())) }
#[tauri::command] fn log_export_csv() -> Result<String, String> { logging::export_csv() }
#[tauri::command] fn log_import_csv(csv: String) -> Result<String, String> { Ok(serde_json::to_string(&logging::import_csv(&csv)?).unwrap_or_else(|_| "{}".into())) }

#[tauri::command]
pub(crate) fn compute_seed_key(seed_hex: String, family: Option<String>, level: Option<String>, algo: Option<u32>) -> Result<String, String> {
    let cleaned: String = seed_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.is_empty() || cleaned.len() % 2 != 0 { return Err("seed_hex must be an even-length hex string".into()); }
    let mut seed = Vec::new();
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i+2]).map_err(|e| e.to_string())?;
        seed.push(u8::from_str_radix(pair, 16).map_err(|e| format!("bad hex: {}", e))?);
        i += 2;
    }
    let fam = family.unwrap_or_else(|| "P01_0411".into());
    let fam_up = fam.to_ascii_uppercase();
    let lvl = level.unwrap_or_else(|| "1".into());

    if let Some(key) = crate::seed_tables::lookup(&fam, &seed, Some(&lvl)) {
        return Ok(json!({
            "family": fam, "level": lvl, "algo": "measured_table", "verified": true,
            "note": "Measured pair from seed_tables.json.",
            "seed_hex": cleaned.to_ascii_uppercase(),
            "key_hex": key.iter().map(|b| format!("{:02X}", b)).collect::<String>(),
            "key_len": key.len(), "gm_table_count": crate::gm_keys::table_count()
        }).to_string());
    }

    if let Some(a) = algo {
        if seed.len() < 2 { return Err("GM 2-byte seed must be at least 2 bytes".into()); }
        let seed_u = u16::from_be_bytes([seed[0], seed[1]]);
        let key_u = crate::gm_keys::gm_2byte_key(a as u16, seed_u);
        let [kh, kl] = key_u.to_be_bytes();
        return Ok(json!({
            "family": fam, "level": lvl, "algo": format!("gm_2byte_{:03X}", a),
            "verified": true,
            "note": "Public 2-byte table algorithm (2byte-keys.txt). Not the licensed 5-byte GM library.",
            "seed_hex": cleaned.to_ascii_uppercase(),
            "key_hex": format!("{:02X}{:02X}", kh, kl),
            "key_len": 2, "gm_algo": a, "gm_table_count": crate::gm_keys::table_count()
        }).to_string());
    }

    if fam_up.contains("P01") || fam_up.contains("P59") || fam_up.contains("GM") {
        if seed.len() < 2 { return Err("P01/P59 seed must be at least 2 bytes".into()); }
        let (kh, kl) = if lvl == "2" { security::p01_key_l2(seed[0], seed[1]) } else { security::p01_key_l1(seed[0], seed[1]) };
        let key = vec![kh, kl];
        return Ok(json!({
            "family":fam,"level":lvl,"algo":"p01_lfsr16","verified":true,
            "note":"GM P01/P59 LFSR. Pass algo (0-1023) to use the 2-byte table set instead.",
            "seed_hex":cleaned.to_ascii_uppercase(),
            "key_hex":key.iter().map(|b| format!("{:02X}", b)).collect::<String>(),
            "key_len":key.len(), "gm_table_count": crate::gm_keys::table_count()
        }).to_string());
    }
    let r = security::bosch_key_result(&seed, &fam);
    Ok(json!({"family":fam,"level":lvl,"algo":r.algo,"verified":r.verified,"note":r.note,"seed_hex":cleaned.to_ascii_uppercase(),"key_hex":r.key.iter().map(|b| format!("{:02X}", b)).collect::<String>(),"key_len":r.key.len()}).to_string())
}

#[tauri::command]
fn gm_key_table_info() -> Result<String, String> {
    Ok(json!({
        "tables": crate::gm_keys::table_count(),
        "algo_min": 0,
        "algo_max": 1023,
        "note": "2-byte GM algorithms only. 5-byte keys need a licensed library we do not ship."
    }).to_string())
}
