// vpw.rs — Enhanced with configurable timeout

// ... existing code ...

/// Send a request and read one response, with configurable timeout and retry.
pub fn request_response(
    port: &mut Box<dyn SerialPort + Send>,
    frame: &[u8],
    timeout_ms: u64,
) -> Result<Vec<u8>, String> {
    send_frame(port, frame)?;

    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(timeout_ms) {
        match recv_frame(port) {
            Ok(resp) if !resp.is_empty() => return Ok(resp),
            Ok(_) => continue,
            Err(e) if e.contains("no data") => continue,
            Err(e) => return Err(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(format!("VPW: no response after {} ms", timeout_ms))
}

// Note on priority: PRIO_HIGH_PHYS and PRIO_FUNC_OBD are defined.
// For full J1850 VPW arbitration, hardware break signal support is recommended on real K-line/VPW adapters.
// Current implementation assumes cooperative bus access.