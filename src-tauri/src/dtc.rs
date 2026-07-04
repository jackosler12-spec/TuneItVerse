// dtc.rs — Enhanced with P3 delay, better error handling, and expanded DTC descriptions

// ... (keep most of the existing code)

pub fn read_dtcs(port: &mut Box<dyn SerialPort + Send>) -> Result<DtcReadResult, String> {
    // Stored DTCs — Mode 03
    let stored = read_dtc_group(port, 0x03, 0x43, false, false)?;
    std::thread::sleep(std::time::Duration::from_millis(55)); // P3 inter-frame delay for P01

    // Pending DTCs — Mode 07
    let pending = read_dtc_group(port, 0x07, 0x47, true, false)?;
    std::thread::sleep(std::time::Duration::from_millis(55));

    // Permanent DTCs — Mode 0A
    let permanent = read_dtc_group(port, 0x0A, 0x4A, false, true)?;

    let total = stored.len() + pending.len() + permanent.len();
    Ok(DtcReadResult { stored, pending, permanent, total })
}

// Improved freeze frame with better error handling
pub fn read_freeze_frame(port: &mut Box<dyn SerialPort + Send>) -> Result<FreezeFrameResult, String> {
    let mut ff = FreezeFrameResult { /* ... default None fields ... */ };
    let mut errors = vec![];

    // PID 0x02 — DTC
    match poll_ff_pid(port, 0x02) {
        Ok(data) => { if data.len() >= 2 { ff.trigger_dtc = decode_dtc_bytes(data[0], data[1]); } }
        Err(e) => errors.push(format!("PID 0x02: {}", e)),
    }

    // ... repeat for other PIDs with error collection ...

    if !errors.is_empty() && ff.trigger_dtc.is_none() {
        // Only fail hard if we got nothing useful
        return Err(format!("Freeze frame partial failure: {}", errors.join("; ")));
    }

    Ok(ff)
}

// Expanded describe_dtc with more B/C/U codes
pub fn describe_dtc(code: &str) -> String {
    match code {
        // ... existing P codes ...
        // Add some common chassis/body/network codes
        "C0035" => "Left Front Wheel Speed Circuit Malfunction",
        "C0040" => "Right Front Wheel Speed Circuit Malfunction",
        "B1000" => "ECU Malfunction",
        "U0100" => "Lost Communication with ECM/PCM",
        "U0101" => "Lost Communication with TCM",
        _ => if code.starts_with('B') { "Body code - consult service manual".to_string() }
             else if code.starts_with('C') { "Chassis code - consult service manual".to_string() }
             else if code.starts_with('U') { "Network code - check communication bus".to_string() }
             else { "No description available".to_string() },
    }
}