//! mock_serial.rs — In-memory SerialPort for integration tests
//!
//! Simulates an ELM327-style adapter:
//!   * AT commands → "OK\r>"
//!   * ISO-TP single-frame UDS requests → positive (or scripted) responses
//!   * Optional FIFO of pre-queued raw read buffers for multi-frame / custom cases
//!
//! Also supports optional live-port tests gated by `TUNEITVERSE_SERIAL_PORT`.

#![allow(dead_code)]

use serialport::{
    ClearBuffer, DataBits, Error, ErrorKind, FlowControl, Parity, Result as SpResult, SerialPort,
    StopBits,
};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::time::Duration;

/// Scripted in-memory port used by integration tests.
pub struct MockSerialPort {
    name: String,
    baud_rate: u32,
    timeout: Duration,
    /// Bytes the DUT has written (for assertions).
    pub written: Vec<u8>,
    /// Bytes available for the next `read` calls.
    read_queue: VecDeque<u8>,
    /// Optional fixed responses pushed ahead of auto-ELM generation.
    scripted: VecDeque<Vec<u8>>,
    /// When true, auto-generate ELM OK / UDS positive responses from writes.
    auto_elm: bool,
}

impl MockSerialPort {
    pub fn new() -> Self {
        Self {
            name: "MOCK".into(),
            baud_rate: 500_000,
            timeout: Duration::from_millis(500),
            written: Vec::new(),
            read_queue: VecDeque::new(),
            scripted: VecDeque::new(),
            auto_elm: true,
        }
    }

    pub fn with_auto_elm(mut self, enabled: bool) -> Self {
        self.auto_elm = enabled;
        self
    }

    /// Queue a raw response that will be drained on the next read(s).
    pub fn push_response(&mut self, data: impl AsRef<[u8]>) {
        self.scripted.push_back(data.as_ref().to_vec());
    }

    /// Queue an ELM-style hex response ending with `\r>`.
    pub fn push_elm_hex(&mut self, payload: &[u8]) {
        let mut s: String = payload.iter().map(|b| format!("{:02X}", b)).collect();
        s.push_str("\r>");
        self.push_response(s.as_bytes());
    }

    /// Queue a plain ELM "OK" prompt.
    pub fn push_ok(&mut self) {
        self.push_response(b"OK\r>");
    }

    /// Bytes written by the DUT since construction / last clear.
    pub fn written_as_str(&self) -> String {
        String::from_utf8_lossy(&self.written).to_string()
    }

    pub fn clear_written(&mut self) {
        self.written.clear();
    }

    fn enqueue_bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.read_queue.push_back(b);
        }
    }

    /// Generate a response for the most recent write line (ends with `\r`).
    fn auto_respond_to_line(&mut self, line: &str) {
        let cmd = line.trim().trim_end_matches('\r').trim();
        if cmd.is_empty() {
            return;
        }

        // Prefer scripted responses if any are pending
        if let Some(resp) = self.scripted.pop_front() {
            self.enqueue_bytes(&resp);
            return;
        }

        if !self.auto_elm {
            return;
        }

        // AT commands
        if cmd.starts_with("AT") || cmd.starts_with("at") {
            self.enqueue_bytes(b"OK\r>");
            return;
        }

        // Hex ISO-TP frame from host
        let cleaned: String = cmd.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if cleaned.len() < 2 {
            self.enqueue_bytes(b"?\r>");
            return;
        }

        let mut frame = Vec::with_capacity(cleaned.len() / 2);
        for i in (0..cleaned.len()).step_by(2) {
            if i + 1 < cleaned.len() {
                if let Ok(b) = u8::from_str_radix(&cleaned[i..i + 2], 16) {
                    frame.push(b);
                }
            }
        }

        if frame.is_empty() {
            self.enqueue_bytes(b"?\r>");
            return;
        }

        // Flow-control frame from host (PCI 0x30) — no ECU payload expected
        if (frame[0] & 0xF0) == 0x30 {
            // CF data may follow in later reads via scripted queue
            return;
        }

        // Single-frame UDS request: PCI_SF | len, then SID + data
        if (frame[0] & 0xF0) == 0x00 {
            let len = (frame[0] & 0x0F) as usize;
            if frame.len() >= 1 + len && len >= 1 {
                let uds = &frame[1..1 + len];
                let resp = synthesize_uds_positive(uds);
                // Wrap as ISO-TP SF
                let mut out = Vec::with_capacity(1 + resp.len());
                out.push(resp.len() as u8); // SF PCI
                out.extend_from_slice(&resp);
                let mut s: String = out.iter().map(|b| format!("{:02X}", b)).collect();
                s.push_str("\r>");
                self.enqueue_bytes(s.as_bytes());
                return;
            }
        }

        // First-frame TX from host — reply with FC CTS
        if (frame[0] & 0xF0) == 0x10 {
            self.enqueue_bytes(b"300000\r>"); // FC CTS BS=0 STmin=0
            return;
        }

        // Consecutive frame — no immediate response
        if (frame[0] & 0xF0) == 0x20 {
            return;
        }

        self.enqueue_bytes(b"?\r>");
    }
}

impl Default for MockSerialPort {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a positive UDS response for common SIDs used in tests.
fn synthesize_uds_positive(request: &[u8]) -> Vec<u8> {
    if request.is_empty() {
        return vec![0x7F, 0x00, 0x10];
    }
    let sid = request[0];
    match sid {
        0x10 => {
            // DiagnosticSessionControl → 0x50 + session
            let session = request.get(1).copied().unwrap_or(0x01);
            vec![0x50, session]
        }
        0x3E => {
            // TesterPresent
            let sub = request.get(1).copied().unwrap_or(0x00);
            if sub & 0x80 != 0 {
                // suppress positive response — return empty (no bytes)
                // ELM still needs something; use minimal OK-style empty SF handled upstream
                vec![0x7E, 0x00]
            } else {
                vec![0x7E, sub & 0x7F]
            }
        }
        0x22 => {
            // RDBI — echo DID + dummy data
            let did_hi = request.get(1).copied().unwrap_or(0);
            let did_lo = request.get(2).copied().unwrap_or(0);
            vec![0x62, did_hi, did_lo, 0xDE, 0xAD]
        }
        0x23 => {
            // RMBA — return a few data bytes
            let mut out = vec![0x63];
            out.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
            out
        }
        0x27 => {
            // SecurityAccess requestSeed → seed; sendKey → empty positive
            let level = request.get(1).copied().unwrap_or(0x01);
            if level % 2 == 1 {
                // request seed
                vec![0x67, level, 0x12, 0x34, 0x56, 0x78]
            } else {
                vec![0x67, level]
            }
        }
        0x28 => {
            let sub = request.get(1).copied().unwrap_or(0);
            vec![0x68, sub]
        }
        0x31 => {
            let sub = request.get(1).copied().unwrap_or(0x01);
            let id_hi = request.get(2).copied().unwrap_or(0);
            let id_lo = request.get(3).copied().unwrap_or(0);
            vec![0x71, sub, id_hi, id_lo]
        }
        0x34 => {
            // RequestDownload → lengthFormat 0x20, max block 0x0402
            vec![0x74, 0x20, 0x04, 0x02]
        }
        0x36 => {
            let seq = request.get(1).copied().unwrap_or(1);
            vec![0x76, seq]
        }
        0x37 => vec![0x77],
        0x11 => {
            let t = request.get(1).copied().unwrap_or(0x01);
            vec![0x51, t]
        }
        0x14 => vec![0x54],
        0x19 => {
            let sub = request.get(1).copied().unwrap_or(0x02);
            vec![0x59, sub, 0xFF] // statusAvailabilityMask
        }
        0x85 => {
            let s = request.get(1).copied().unwrap_or(0x01);
            vec![0xC5, s]
        }
        _ => vec![0x7F, sid, 0x11], // serviceNotSupported
    }
}

impl Read for MockSerialPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.read_queue.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "mock serial: no data",
            ));
        }
        let n = buf.len().min(self.read_queue.len());
        for i in 0..n {
            buf[i] = self.read_queue.pop_front().unwrap();
        }
        Ok(n)
    }
}

impl Write for MockSerialPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.written.extend_from_slice(buf);
        // Process complete lines ending in \r
        let text = String::from_utf8_lossy(buf);
        for line in text.split_inclusive('\r') {
            if line.contains('\r') {
                self.auto_respond_to_line(line);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SerialPort for MockSerialPort {
    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn baud_rate(&self) -> SpResult<u32> {
        Ok(self.baud_rate)
    }

    fn data_bits(&self) -> SpResult<DataBits> {
        Ok(DataBits::Eight)
    }

    fn flow_control(&self) -> SpResult<FlowControl> {
        Ok(FlowControl::None)
    }

    fn parity(&self) -> SpResult<Parity> {
        Ok(Parity::None)
    }

    fn stop_bits(&self) -> SpResult<StopBits> {
        Ok(StopBits::One)
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }

    fn set_baud_rate(&mut self, baud_rate: u32) -> SpResult<()> {
        self.baud_rate = baud_rate;
        Ok(())
    }

    fn set_data_bits(&mut self, _data_bits: DataBits) -> SpResult<()> {
        Ok(())
    }

    fn set_flow_control(&mut self, _flow_control: FlowControl) -> SpResult<()> {
        Ok(())
    }

    fn set_parity(&mut self, _parity: Parity) -> SpResult<()> {
        Ok(())
    }

    fn set_stop_bits(&mut self, _stop_bits: StopBits) -> SpResult<()> {
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> SpResult<()> {
        self.timeout = timeout;
        Ok(())
    }

    fn write_request_to_send(&mut self, _level: bool) -> SpResult<()> {
        Ok(())
    }

    fn write_data_terminal_ready(&mut self, _level: bool) -> SpResult<()> {
        Ok(())
    }

    fn read_clear_to_send(&mut self) -> SpResult<bool> {
        Ok(true)
    }

    fn read_data_set_ready(&mut self) -> SpResult<bool> {
        Ok(true)
    }

    fn read_ring_indicator(&mut self) -> SpResult<bool> {
        Ok(false)
    }

    fn read_carrier_detect(&mut self) -> SpResult<bool> {
        Ok(true)
    }

    fn bytes_to_read(&self) -> SpResult<u32> {
        Ok(self.read_queue.len() as u32)
    }

    fn bytes_to_write(&self) -> SpResult<u32> {
        Ok(0)
    }

    fn clear(&self, _buffer_to_clear: ClearBuffer) -> SpResult<()> {
        Ok(())
    }

    fn try_clone(&self) -> SpResult<Box<dyn SerialPort>> {
        Err(Error::new(
            ErrorKind::Unknown,
            "MockSerialPort cannot be cloned",
        ))
    }

    fn set_break(&self) -> SpResult<()> {
        Ok(())
    }

    fn clear_break(&self) -> SpResult<()> {
        Ok(())
    }
}

/// Helper: wrap mock as `Box<dyn SerialPort + Send>` for APIs that take that type.
pub fn boxed_mock(mock: MockSerialPort) -> Box<dyn SerialPort + Send> {
    Box::new(mock)
}

/// Open a real serial port when `TUNEITVERSE_SERIAL_PORT` is set (hardware integration).
pub fn try_open_live_port() -> Option<Box<dyn SerialPort + Send>> {
    let path = std::env::var("TUNEITVERSE_SERIAL_PORT").ok()?;
    if path.trim().is_empty() {
        return None;
    }
    serialport::new(&path, 500_000)
        .timeout(Duration::from_millis(800))
        .open()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_ok_for_at_commands() {
        let mut p = MockSerialPort::new();
        p.write_all(b"AT Z\r").unwrap();
        let mut buf = [0u8; 16];
        let n = p.read(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);
        assert!(s.contains("OK"));
    }

    #[test]
    fn auto_session_control_positive() {
        let mut p = MockSerialPort::new();
        // SF: len=2, SID=0x10, session=0x03
        p.write_all(b"021003\r").unwrap();
        let mut buf = [0u8; 32];
        let n = p.read(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);
        // Expect SF 0x02 0x50 0x03
        assert!(s.contains("025003") || s.contains("50"));
    }

    #[test]
    fn scripted_response_overrides_auto() {
        let mut p = MockSerialPort::new();
        p.push_response(b"7F1022\r>");
        p.write_all(b"021003\r").unwrap();
        let mut buf = [0u8; 32];
        let n = p.read(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);
        assert!(s.contains("7F1022"));
    }

    #[test]
    fn written_captured() {
        let mut p = MockSerialPort::new();
        p.write_all(b"HELLO\r").unwrap();
        assert!(p.written_as_str().contains("HELLO"));
    }
}
