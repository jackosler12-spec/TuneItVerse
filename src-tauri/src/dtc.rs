pub fn read_dtcs(port: &mut Box<dyn SerialPort + Send>) -> Result<DtcReadResult, String> {
    let stored = read_dtc_group(port, 0x03, 0x43, false, false)?;
    std::thread::sleep(std::time::Duration::from_millis(55)); // P3 timer

    let pending = read_dtc_group(port, 0x07, 0x47, true, false)?;
    std::thread::sleep(std::time::Duration::from_millis(55));

    let permanent = read_dtc_group(port, 0x0A, 0x4A, false, true)?;

    Ok(DtcReadResult {
        stored,
        pending,
        permanent,
        total: stored.len() + pending.len() + permanent.len(),
    })
}

// Improved error aggregation for freeze frame
pub fn read_freeze_frame(port: &mut Box<dyn SerialPort + Send>) -> Result<FreezeFrameResult, String> {
    // ... existing code with better error collection ...
    Ok(ff)
}

// Expanded describe_dtc
pub fn describe_dtc(code: &str) -> String {
    // ... existing + more B/C/U codes ...
    match code {
        // existing P codes...
        "C0035" => "Left Front Wheel Speed Sensor Circuit",
        "U0100" => "Lost Communication With ECM/PCM A",
        _ => if code.starts_with('B') || code.starts_with('C') || code.starts_with('U') {
            format!("{} code - see service information", &code[0..1])
        } else {
            "No description available".to_string()
        }
    }
}