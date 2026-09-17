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
        std::thread::sleep(Duration::from_millis(80));
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
    Ok(format!("Connected to {} @ {} baud ({})", port_name, baud, protocol))
}
#[tauri::command]
fn disconnect_ecu() -> Result<String, String> {
    let _ = crate::j2534::j2534_disconnect();
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
        let mut guard = STATE.lock().map_err(|e| e.to_string())?;
        guard.port = Some(port);
        guard.protocol = "vpw".into();
        return Ok("Detected: VPW/J1850 (Mode 01 header)".into());
    }
    Err("No adapter response. Check port, baud, and that the interface is powered.".into())
}
