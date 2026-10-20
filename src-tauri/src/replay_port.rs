#![allow(dead_code)]
//! replay_port.rs — scripted ELM/STN serial port for tests and offline replay.
//!
//! Fed from a redacted fixture (`reference/fixtures/*.txt`, lines
//! `> REQUEST  ||  < RESPONSE`, multi-line replies joined with ` | `). Each
//! request replays its recorded replies in order and then repeats the last one.
//! Requests that are not in the fixture get what a real adapter would print:
//! `NO DATA` for hex OBD requests, `?` for anything else. Nothing is invented.

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared log of commands written to the port (CR stripped).
#[derive(Clone, Default)]
pub struct SentLog(Arc<Mutex<Vec<String>>>);

impl SentLog {
    pub fn get(&self) -> Vec<String> {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

pub struct ReplayPort {
    script: HashMap<String, VecDeque<String>>,
    last: HashMap<String, String>,
    rx: Mutex<VecDeque<u8>>,
    pending_cmd: Vec<u8>,
    sent: SentLog,
}

fn normalize(cmd: &str) -> String {
    cmd.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

impl ReplayPort {
    pub fn empty() -> Self {
        Self { script: HashMap::new(), last: HashMap::new(), rx: Mutex::new(VecDeque::new()), pending_cmd: Vec::new(), sent: SentLog::default() }
    }

    /// Load the exchanges under the section whose heading starts with `section`
    /// (e.g. `"## Section A1"`), up to the next `## ` heading. Empty = whole file.
    pub fn from_fixture(text: &str, section: &str) -> Self {
        let mut port = Self::empty();
        let mut inside = section.is_empty();
        for line in text.lines() {
            if line.starts_with("## ") {
                inside = section.is_empty() || line.starts_with(section);
                continue;
            }
            if !inside {
                continue;
            }
            let Some(rest) = line.strip_prefix("> ") else { continue };
            let Some((req, resp)) = rest.split_once("||") else { continue };
            let resp = resp.trim().strip_prefix('<').unwrap_or(resp).trim();
            // Drop "(xN)" repeat markers.
            let resp = match resp.rfind("(x") {
                Some(i) if resp.ends_with(')') => resp[..i].trim(),
                _ => resp,
            };
            let reply = resp.split(" | ").map(str::trim).collect::<Vec<_>>().join("\r");
            port.script.entry(normalize(req)).or_default().push_back(reply);
        }
        port
    }

    /// Add (or append) replies for a command. Multi-line replies use `\r`.
    pub fn with_reply(mut self, cmd: &str, replies: &[&str]) -> Self {
        let q = self.script.entry(normalize(cmd)).or_default();
        for r in replies {
            q.push_back(r.to_string());
        }
        self
    }

    pub fn sent_log(&self) -> SentLog {
        self.sent.clone()
    }

    pub fn boxed(self) -> (Box<dyn SerialPort + Send>, SentLog) {
        let log = self.sent_log();
        (Box::new(self), log)
    }

    fn take_reply(&mut self, key: &str) -> Option<String> {
        if let Some(q) = self.script.get_mut(key) {
            if let Some(r) = q.pop_front() {
                self.last.insert(key.to_string(), r.clone());
                return Some(r);
            }
        }
        self.last.get(key).cloned()
    }

    fn answer(&mut self, cmd: &str) -> String {
        let key = normalize(cmd);
        let is_hex = !key.is_empty() && key.chars().all(|c| c.is_ascii_hexdigit());
        let mut reply = self.take_reply(&key);
        // ELM response-count suffix: "010C1" and "010C" ask the same question.
        if reply.is_none() && is_hex && key.len() % 2 == 1 && key.len() > 2 {
            reply = self.take_reply(&key[..key.len() - 1]);
        }
        if reply.is_none() && is_hex && key.len() % 2 == 0 {
            reply = self.take_reply(&format!("{}1", key));
        }
        let body = reply.unwrap_or_else(|| if is_hex { "NO DATA".into() } else { "?".into() });
        format!("{}\r\r>", body)
    }
}

impl Read for ReplayPort {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut rx = self.rx.lock().unwrap();
        if rx.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "replay: no data"));
        }
        let n = buf.len().min(rx.len());
        for b in buf.iter_mut().take(n) {
            *b = rx.pop_front().unwrap_or(0);
        }
        Ok(n)
    }
}

impl Write for ReplayPort {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &b in buf {
            if b == b'\r' {
                let cmd = String::from_utf8_lossy(&self.pending_cmd).trim().to_string();
                self.pending_cmd.clear();
                if cmd.is_empty() {
                    continue;
                }
                if let Ok(mut s) = self.sent.0.lock() {
                    s.push(normalize(&cmd));
                }
                let ans = self.answer(&cmd);
                self.rx.lock().unwrap().extend(ans.bytes());
            } else {
                self.pending_cmd.push(b);
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SerialPort for ReplayPort {
    fn name(&self) -> Option<String> { Some("replay".into()) }
    fn baud_rate(&self) -> serialport::Result<u32> { Ok(115_200) }
    fn data_bits(&self) -> serialport::Result<DataBits> { Ok(DataBits::Eight) }
    fn flow_control(&self) -> serialport::Result<FlowControl> { Ok(FlowControl::None) }
    fn parity(&self) -> serialport::Result<Parity> { Ok(Parity::None) }
    fn stop_bits(&self) -> serialport::Result<StopBits> { Ok(StopBits::One) }
    fn timeout(&self) -> Duration { Duration::from_millis(1) }
    fn set_baud_rate(&mut self, _: u32) -> serialport::Result<()> { Ok(()) }
    fn set_data_bits(&mut self, _: DataBits) -> serialport::Result<()> { Ok(()) }
    fn set_flow_control(&mut self, _: FlowControl) -> serialport::Result<()> { Ok(()) }
    fn set_parity(&mut self, _: Parity) -> serialport::Result<()> { Ok(()) }
    fn set_stop_bits(&mut self, _: StopBits) -> serialport::Result<()> { Ok(()) }
    fn set_timeout(&mut self, _: Duration) -> serialport::Result<()> { Ok(()) }
    fn write_request_to_send(&mut self, _: bool) -> serialport::Result<()> { Ok(()) }
    fn write_data_terminal_ready(&mut self, _: bool) -> serialport::Result<()> { Ok(()) }
    fn read_clear_to_send(&mut self) -> serialport::Result<bool> { Ok(true) }
    fn read_data_set_ready(&mut self) -> serialport::Result<bool> { Ok(true) }
    fn read_ring_indicator(&mut self) -> serialport::Result<bool> { Ok(false) }
    fn read_carrier_detect(&mut self) -> serialport::Result<bool> { Ok(true) }
    fn bytes_to_read(&self) -> serialport::Result<u32> { Ok(self.rx.lock().map(|r| r.len() as u32).unwrap_or(0)) }
    fn bytes_to_write(&self) -> serialport::Result<u32> { Ok(0) }
    fn clear(&self, which: ClearBuffer) -> serialport::Result<()> {
        if matches!(which, ClearBuffer::Input | ClearBuffer::All) {
            if let Ok(mut rx) = self.rx.lock() {
                rx.clear();
            }
        }
        Ok(())
    }
    fn try_clone(&self) -> serialport::Result<Box<dyn SerialPort>> {
        Err(serialport::Error::new(serialport::ErrorKind::Unknown, "replay port cannot be cloned"))
    }
    fn set_break(&self) -> serialport::Result<()> { Ok(()) }
    fn clear_break(&self) -> serialport::Result<()> { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../../reference/fixtures/elm_patrol_zd30_obdlink.txt");

    #[test]
    fn fixture_is_scrubbed() {
        let up = FIXTURE.to_ascii_uppercase();
        // No MAC-address pattern, no adapter serial / phone / GPS fields.
        let bytes: Vec<char> = up.chars().collect();
        for w in bytes.windows(17) {
            let s: String = w.iter().collect();
            let mac = s.split(':').count() == 6 && s.split(':').all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!mac, "MAC-like text in fixture: {s}");
        }
        for banned in ["LATITUDE", "LONGTITUDE", "DEVICENAME", "SERIAL NUMBER", "STSN"] {
            assert!(!up.contains(banned), "{banned}");
        }
    }

    #[test]
    fn replays_in_order_then_repeats() {
        let (mut p, log) = ReplayPort::from_fixture(FIXTURE, "## Section A1").boxed();
        p.write_all(b"0100\r").unwrap();
        let mut buf = [0u8; 128];
        let n = p.read(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("SEARCHING...\r7E806410098390011"));
        p.write_all(b"ATL0\r").unwrap();
        let n = p.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"?\r\r>");
        p.write_all(b"0142\r").unwrap();
        let n = p.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"NO DATA\r\r>");
        assert_eq!(log.get(), vec!["0100", "ATL0", "0142"]);
    }
}
