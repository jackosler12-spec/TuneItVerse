pub fn write_frame(port: &mut Box<dyn SerialPort + Send>, frame: &[u8]) -> Result<(), String> {
    port.write_all(frame).map_err(|e| format!("Write failed: {}", e))?;
    port.flush().map_err(|e| format!("Flush failed: {}", e))?;
    Ok(())
}

pub fn read_response(port: &mut Box<dyn SerialPort + Send>) -> Result<Vec<u8>, String> {
    let mut buf = [0u8; 512];
    match port.read(&mut buf) {
        Ok(n) if n > 0 => Ok(buf[..n].to_vec()),
        Ok(_) => Err("Empty response".into()),
        Err(e) => Err(format!("Read failed: {}", e)),
    }
}

pub fn validate_checksum(frame: &[u8]) -> bool {
    if frame.len() < 2 { return false; }
    let expected = frame[..frame.len() - 1].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    expected == frame[frame.len() - 1]
}

#[tauri::command]
fn list_serial_ports() -> Result<Vec<String>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;
    Ok(ports.into_iter().map(|p| p.port_name).collect())
}
#[tauri::command]
fn get_connection_health() -> Result<String, String> {
    if crate::j2534::is_device_open() {
        return Ok("Connected (j2534)".into());
    }
    let guard = STATE.lock().map_err(|e| e.to_string())?;
    if guard.port.is_some() { Ok(format!("Connected ({})", guard.protocol)) } else { Ok("Disconnected".into()) }
}
fn elm_warmup(port: &mut dyn SerialPort, protocol: &str) {
    let proto = protocol.to_ascii_lowercase();
    let seq: &[&[u8]] = if proto.contains("uds") || proto.contains("can") {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP6\r"]
    } else if proto.contains("kwp") {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP5\r"]
    } else if proto.contains("vpw") {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP2\r"]
    } else {
        &[b"ATZ\r", b"ATE0\r", b"ATL0\r", b"ATS0\r", b"ATH1\r"]
    };
    for cmd in seq {
        let _ = port.write_all(cmd);
        // ATZ is a full adapter reset (~1 s on ELM327/STN); commands sent during it are lost.
        std::thread::sleep(Duration::from_millis(if cmd.starts_with(b"ATZ") { 1000 } else { 80 }));
        let _ = port.clear(serialport::ClearBuffer::Input);
    }
}

#[tauri::command]
fn connect_ecu(port_name: String, baud: u32, protocol: String) -> Result<String, String> {
    let opened = serialport::new(&port_name, baud).timeout(Duration::from_millis(500)).open()
        .map_err(|e| format!("Failed to open {}: {}", port_name, e))?;
    // serialport 4 returns Box<dyn SerialPort>; AppState requires Send. Windows COM ports are Send.
    let mut port: Box<dyn SerialPort + Send> =
        unsafe { std::mem::transmute::<Box<dyn SerialPort>, Box<dyn SerialPort + Send>>(opened) };
    elm_warmup(port.as_mut(), &protocol);
    // New adapter session: clears the cached PID support masks and NO DATA streak.
    crate::transport::begin_session(&protocol);

    // Prefer OBDLink STP path when we know the protocol and the adapter answers ST commands.
    let mut extra = String::new();
    if let Some(stp) = crate::transport::protocol_to_stp(&protocol) {
        // Try ST setup; ignore failure so pure ELM clones still work with the ATSP warmup.
        if let Ok(v) = crate::transport::setup_obdlink(&mut port, stp) {
            extra = format!(" (OBDLink STP{} {:.1}V)", stp, v);
            // For VPW/P01, apply a common PCM response filter on header 0x10.
            if stp == "12" {
                let _ = crate::transport::clear_filters(&mut port);
                let _ = crate::transport::set_pass_filter(&mut port, "10");
            }
            // For CAN/UDS (STP 33), set a common 11-bit functional response filter.
            if stp == "33" {
                let _ = crate::transport::clear_filters(&mut port);
                let _ = crate::transport::set_flow_control_filter(&mut port, "7E8");
            }
        }
    }

    let proto_l = protocol.to_ascii_lowercase();
    if proto_l.contains("can") || proto_l.contains("uds") {
        let _ = crate::can::elm_init_can_500k(&mut port);
    } else if proto_l.contains("consult") {
        let _ = crate::consult::consult_init(&mut port);
    } else if proto_l.contains("kwp") {
        let _ = crate::kwp::kwp_fast_init(&mut port);
    }
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = Some(port);
    guard.protocol = protocol.clone();
    Ok(format!("Connected to {} @ {} baud ({}){}", port_name, baud, protocol, extra))
}
#[tauri::command]
fn disconnect_ecu() -> Result<String, String> {
    let _ = crate::j2534::j2534_disconnect();
    let _ = crate::with_port(|port| { crate::consult::nissan_block_end(port); Ok(()) });
    crate::transport::end_session();
    let mut guard = STATE.lock().map_err(|e| e.to_string())?;
    guard.port = None; guard.protocol = String::new(); guard.last_os_id = None; guard.last_family = None;
    Ok("Disconnected".into())
}

fn read_ascii_window(port: &mut dyn SerialPort, wait_ms: u64) -> String {
    std::thread::sleep(Duration::from_millis(wait_ms));
    let mut buf = [0u8; 128];
    match port.read(&mut buf) {
        Ok(n) if n > 0 => String::from_utf8_lossy(&buf[..n]).to_ascii_uppercase(),
        _ => String::new(),
    }
}

#[tauri::command]
fn auto_detect_protocol(port_name: String) -> Result<String, String> {
    let mut port = serialport::new(&port_name, 115200).timeout(Duration::from_millis(400)).open()
        .map_err(|e| format!("Failed to open {}: {}", port_name, e))?;
    let _ = port.write_all(b"ATZ\r");
    let ident = read_ascii_window(port.as_mut(), 200);
    let elm_like = ident.contains("ELM") || ident.contains("OBD") || ident.contains("STN") || ident.contains("OK");
    if elm_like {
        for cmd in [b"ATE0\r".as_slice(), b"ATL0\r", b"ATS0\r", b"ATH1\r", b"ATSP0\r"] {
            let _ = port.write_all(cmd);
            let _ = read_ascii_window(port.as_mut(), 80);
        }
        let _ = port.write_all(b"0100\r");
        let pid = read_ascii_window(port.as_mut(), 300);
        if pid.contains("41 00") || pid.contains("4100") || pid.contains("UNABLE") {
            let proto = if pid.contains("41") { "elm-auto (Mode 01 seen)" } else { "elm (adapter answered, no PID yet)" };
            crate::transport::begin_session(proto);
            let mut guard = STATE.lock().map_err(|e| e.to_string())?;
            guard.port = Some(port);
            guard.protocol = proto.into();
            return Ok(format!("Detected: {}", proto));
        }
        drop(port);
        return Err("ELM-like adapter answered ATZ but Mode 01 PID 00 did not. Check ignition and protocol.".into());
    }
    drop(port);
    let mut port = serialport::new(&port_name, 10400).timeout(Duration::from_millis(400)).open()
        .map_err(|e| format!("Failed to reopen {} at 10400: {}", port_name, e))?;
    let _ = port.write_all(&[0x68, 0x6A, 0xF1, 0x01, 0x00, 0xC4]);
    std::thread::sleep(Duration::from_millis(120));
    let mut buf = [0u8; 64];
    let n = port.read(&mut buf).unwrap_or(0);
    if n >= 5 && (buf[0] == 0x48 || buf[0] == 0x41) {
        crate::transport::begin_session("vpw");
        let mut guard = STATE.lock().map_err(|e| e.to_string())?;
        guard.port = Some(port);
        guard.protocol = "vpw".into();
        return Ok("Detected: VPW/J1850 (Mode 01 header)".into());
    }
    Err("No adapter response. Check port, baud, and that the interface is powered.".into())
}

// ── ST / OBDLink helpers exposed to the UI ──

#[tauri::command]
fn st_identify() -> Result<String, String> {
    with_port(|port| Ok(crate::transport::identify_adapter(port)))
}

#[tauri::command]
fn st_read_voltage() -> Result<f32, String> {
    with_port(|port| crate::transport::read_adapter_voltage(port).ok_or_else(|| "No STVR/ATRV".into()))
}

#[tauri::command]
fn st_set_protocol(stp: String) -> Result<String, String> {
    with_port(|port| crate::transport::set_protocol(port, &stp))
}

#[tauri::command]
fn st_open_protocol() -> Result<String, String> {
    with_port(|port| crate::transport::open_protocol(port))
}

#[tauri::command]
fn st_set_timeout(ms: u32) -> Result<String, String> {
    with_port(|port| crate::transport::set_timeout_ms(port, ms))
}

#[tauri::command]
fn st_clear_filters() -> Result<String, String> {
    with_port(|port| crate::transport::clear_filters(port))
}

#[tauri::command]
fn st_set_pass_filter(id: String) -> Result<String, String> {
    with_port(|port| crate::transport::set_pass_filter(port, &id))
}

#[tauri::command]
fn st_set_flow_control_filter(id: String) -> Result<String, String> {
    with_port(|port| crate::transport::set_flow_control_filter(port, &id))
}

#[tauri::command]
fn st_start_monitor() -> Result<String, String> {
    with_port(|port| crate::transport::start_monitor(port))
}

#[tauri::command]
fn st_stop_monitor() -> Result<String, String> {
    with_port(|port| crate::transport::stop_monitor(port))
}

/// Capture a short bus dump (STM for `ms` milliseconds). Default 500 ms.
#[tauri::command]
fn st_capture_monitor(ms: Option<u64>) -> Result<String, String> {
    let ms = ms.unwrap_or(500).min(3000);
    with_port(|port| crate::transport::capture_monitor(port, ms))
}

#[tauri::command]
fn st_enter_sleep() -> Result<String, String> {
    with_port(|port| crate::transport::enter_sleep(port))
}

#[tauri::command]
fn st_set_sleep_wake(mode: u8) -> Result<String, String> {
    with_port(|port| crate::transport::set_sleep_wake(port, mode))
}

#[tauri::command]
fn st_read_pp(pp: u8) -> Result<String, String> {
    with_port(|port| crate::transport::read_pp(port, pp))
}

#[tauri::command]
fn st_write_pp(pp: u8, value: u8) -> Result<String, String> {
    with_port(|port| crate::transport::write_pp(port, pp, value))
}

#[tauri::command]
fn st_stpx(hex: String) -> Result<String, String> {
    let bytes = crate::transport::parse_elm_hex(&hex);
    if bytes.is_empty() {
        return Err("STPX: no hex bytes".into());
    }
    with_port(|port| {
        let resp = crate::transport::stpx(port, &bytes)?;
        Ok(resp.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" "))
    })
}

/// Combined adapter snapshot: identity, voltage, and current protocol.
/// Returns JSON so the UI can show it without multiple round-trips.
#[tauri::command]
fn st_adapter_status() -> Result<String, String> {
    with_port(|port| {
        let id = crate::transport::identify_adapter(port);
        let v = crate::transport::read_adapter_voltage(port);
        let proto = crate::transport::current_protocol();
        let obj = serde_json::json!({
            "identity": id,
            "voltage": v,
            "protocol": proto,
        });
        Ok(obj.to_string())
    })
}

/// Cached adapter session: supported Mode 01 PIDs, protocol search state, NO DATA streak.
#[tauri::command]
fn elm_session_info() -> Result<String, String> {
    Ok(crate::transport::session_snapshot().to_string())
}

/// Read-only Nissan CAN diagnostic scan (ATSH7E0, 10C0, 22xxxx support masks,
/// optional raw reads, 1081). ID meanings are unknown; raw bytes only.
#[tauri::command]
fn nissan_can_scan(read_values: Option<bool>) -> Result<String, String> {
    with_port(|port| {
        let scan = crate::consult::nissan_can_scan(port, read_values.unwrap_or(false))?;
        Ok(serde_json::to_string(&scan).unwrap_or_else(|_| "{}".into()))
    })
}
