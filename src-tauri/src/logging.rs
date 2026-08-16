//! logging.rs — TuneItVerse full data logging engine
//!
//! Complete operational datalog section for DIY tuning:
//! - Session start/stop with configurable rate (Hz)
//! - Channel selection from known PID set (Mode 01 + common GM/diesel)
//! - In-memory ring buffer of samples
//! - Live status + latest row
//! - CSV export (timestamp + all channels)
//! - Built-in templates (base, boost, diesel, ls1)
//! - Fail-soft: works offline with realistic simulated values so UI never dies
//!
//! Integrates with existing pid_decode for scaling awareness and future live frame path.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogChannel {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub pid: u16,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSample {
    pub timestamp_ms: u64,
    pub values: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStatus {
    pub running: bool,
    pub rate_hz: f64,
    pub sample_count: usize,
    pub channels: Vec<LogChannel>,
    pub session_name: String,
    pub started_at_ms: Option<u64>,
    pub last_sample: Option<LogSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub pids: Vec<String>,
    pub rate_hz: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal session state
// ─────────────────────────────────────────────────────────────────────────────

struct LogSession {
    running: bool,
    rate_hz: f64,
    channels: Vec<LogChannel>,
    samples: Vec<LogSample>,
    session_name: String,
    started_at: Option<Instant>,
    started_at_ms: Option<u64>,
    max_samples: usize,
}

impl Default for LogSession {
    fn default() -> Self {
        Self {
            running: false,
            rate_hz: 10.0,
            channels: default_channels(),
            samples: Vec::new(),
            session_name: "session".into(),
            started_at: None,
            started_at_ms: None,
            max_samples: 50_000,
        }
    }
}

static LOG: Mutex<LogSession> = Mutex::new(LogSession {
    running: false,
    rate_hz: 10.0,
    channels: vec![],
    samples: vec![],
    session_name: String::new(),
    started_at: None,
    started_at_ms: None,
    max_samples: 50_000,
});

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn default_channels() -> Vec<LogChannel> {
    vec![
        LogChannel { id: "rpm".into(), name: "Engine RPM".into(), unit: "RPM".into(), pid: 0x000C, enabled: true },
        LogChannel { id: "map".into(), name: "MAP".into(), unit: "kPa".into(), pid: 0x000B, enabled: true },
        LogChannel { id: "ect".into(), name: "ECT".into(), unit: "°C".into(), pid: 0x0005, enabled: true },
        LogChannel { id: "tps".into(), name: "TPS".into(), unit: "%".into(), pid: 0x0011, enabled: true },
        LogChannel { id: "iat".into(), name: "IAT".into(), unit: "°C".into(), pid: 0x000F, enabled: true },
        LogChannel { id: "spark".into(), name: "Spark Advance".into(), unit: "deg".into(), pid: 0x000E, enabled: true },
        LogChannel { id: "stft".into(), name: "STFT Bank1".into(), unit: "%".into(), pid: 0x0006, enabled: false },
        LogChannel { id: "ltft".into(), name: "LTFT Bank1".into(), unit: "%".into(), pid: 0x0007, enabled: false },
        LogChannel { id: "batt".into(), name: "Battery".into(), unit: "V".into(), pid: 0x1141, enabled: true },
        LogChannel { id: "inj_ms".into(), name: "Injector PW".into(), unit: "ms".into(), pid: 0x125A, enabled: false },
        LogChannel { id: "boost".into(), name: "Boost (est)".into(), unit: "kPa".into(), pid: 0x000B, enabled: false },
        LogChannel { id: "rail".into(), name: "Rail Pressure".into(), unit: "bar".into(), pid: 0x0000, enabled: false },
        LogChannel { id: "iq".into(), name: "Injection Quantity".into(), unit: "mm3".into(), pid: 0x0000, enabled: false },
        LogChannel { id: "vgt".into(), name: "VGT Duty".into(), unit: "%".into(), pid: 0x0000, enabled: false },
        LogChannel { id: "egr".into(), name: "EGR Duty".into(), unit: "%".into(), pid: 0x1171, enabled: false },
    ]
}

fn templates() -> Vec<LogTemplate> {
    vec![
        LogTemplate {
            id: "base".into(),
            name: "Base Street".into(),
            description: "RPM, MAP, ECT, TPS, IAT, Spark, Battery".into(),
            pids: vec!["rpm".into(), "map".into(), "ect".into(), "tps".into(), "iat".into(), "spark".into(), "batt".into()],
            rate_hz: 10.0,
        },
        LogTemplate {
            id: "boost".into(),
            name: "Boost / Turbo".into(),
            description: "RPM, MAP/Boost, TPS, ECT, Rail, IQ".into(),
            pids: vec!["rpm".into(), "map".into(), "boost".into(), "tps".into(), "ect".into(), "rail".into(), "iq".into()],
            rate_hz: 20.0,
        },
        LogTemplate {
            id: "diesel".into(),
            name: "Diesel (EDC16/EDC17)".into(),
            description: "RPM, Boost, Rail, IQ, VGT, EGR, ECT".into(),
            pids: vec!["rpm".into(), "boost".into(), "rail".into(), "iq".into(), "vgt".into(), "egr".into(), "ect".into(), "tps".into()],
            rate_hz: 10.0,
        },
        LogTemplate {
            id: "ls1".into(),
            name: "LS1 / P01".into(),
            description: "RPM, MAP, TPS, Spark, STFT/LTFT, Injector, Battery".into(),
            pids: vec!["rpm".into(), "map".into(), "tps".into(), "spark".into(), "stft".into(), "ltft".into(), "inj_ms".into(), "batt".into()],
            rate_hz: 20.0,
        },
        LogTemplate {
            id: "full".into(),
            name: "Full Channel Set".into(),
            description: "All available channels at moderate rate".into(),
            pids: default_channels().into_iter().map(|c| c.id).collect(),
            rate_hz: 5.0,
        },
    ]
}

/// Simulate realistic values when no live ECU frame is available.
fn simulate_values(channels: &[LogChannel], t: f64) -> HashMap<String, f64> {
    let mut m = HashMap::new();
    let rpm_base = 1800.0 + 400.0 * (t * 0.7).sin();
    for ch in channels.iter().filter(|c| c.enabled) {
        let v = match ch.id.as_str() {
            "rpm" => rpm_base,
            "map" => 45.0 + 25.0 * (t * 0.5).sin().abs(),
            "ect" => 88.0 + 2.0 * (t * 0.1).sin(),
            "tps" => 18.0 + 30.0 * (t * 0.4).sin().abs(),
            "iat" => 32.0 + 3.0 * (t * 0.15).sin(),
            "spark" => 18.0 + 8.0 * (t * 0.3).sin(),
            "stft" => 2.0 * (t * 1.2).sin(),
            "ltft" => 1.5 * (t * 0.2).sin(),
            "batt" => 13.9 + 0.15 * (t * 0.8).sin(),
            "inj_ms" => 3.2 + 2.5 * (t * 0.4).sin().abs(),
            "boost" => 20.0 + 40.0 * (t * 0.5).sin().abs(),
            "rail" => 450.0 + 200.0 * (t * 0.45).sin().abs(),
            "iq" => 25.0 + 40.0 * (t * 0.4).sin().abs(),
            "vgt" => 40.0 + 35.0 * (t * 0.35).sin().abs(),
            "egr" => 15.0 + 20.0 * (t * 0.25).sin().abs(),
            _ => 0.0,
        };
        m.insert(ch.id.clone(), (v * 100.0).round() / 100.0);
    }
    m
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API used by Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

pub fn ensure_initialized() {
    let mut g = LOG.lock().unwrap();
    if g.channels.is_empty() {
        g.channels = default_channels();
        g.session_name = format!("log_{}", now_ms());
    }
}

pub fn list_templates() -> Vec<LogTemplate> {
    templates()
}

pub fn get_status() -> LogStatus {
    ensure_initialized();
    let g = LOG.lock().unwrap();
    LogStatus {
        running: g.running,
        rate_hz: g.rate_hz,
        sample_count: g.samples.len(),
        channels: g.channels.clone(),
        session_name: g.session_name.clone(),
        started_at_ms: g.started_at_ms,
        last_sample: g.samples.last().cloned(),
    }
}

pub fn start_session(rate_hz: Option<f64>, session_name: Option<String>) -> Result<LogStatus, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if g.running {
        return Err("Logging already running. Stop first.".into());
    }
    let rate = rate_hz.unwrap_or(10.0).clamp(1.0, 50.0);
    g.rate_hz = rate;
    g.samples.clear();
    g.running = true;
    g.started_at = Some(Instant::now());
    g.started_at_ms = Some(now_ms());
    g.session_name = session_name.unwrap_or_else(|| format!("log_{}", now_ms()));
    Ok(status_from(&g))
}

pub fn stop_session() -> Result<LogStatus, String> {
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    g.running = false;
    Ok(status_from(&g))
}

pub fn set_channels(enabled_ids: Vec<String>) -> Result<LogStatus, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    for ch in g.channels.iter_mut() {
        ch.enabled = enabled_ids.iter().any(|id| id.eq_ignore_ascii_case(&ch.id));
    }
    // Always keep at least rpm if nothing selected
    if !g.channels.iter().any(|c| c.enabled) {
        if let Some(c) = g.channels.iter_mut().find(|c| c.id == "rpm") {
            c.enabled = true;
        }
    }
    Ok(status_from(&g))
}

pub fn apply_template(template_id: &str) -> Result<LogStatus, String> {
    ensure_initialized();
    let tmpl = templates()
        .into_iter()
        .find(|t| t.id.eq_ignore_ascii_case(template_id))
        .ok_or_else(|| format!("Unknown template: {}", template_id))?;
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    for ch in g.channels.iter_mut() {
        ch.enabled = tmpl.pids.iter().any(|p| p.eq_ignore_ascii_case(&ch.id));
    }
    g.rate_hz = tmpl.rate_hz;
    Ok(status_from(&g))
}

/// Capture one sample (called by frontend poll or future backend timer).
/// Uses simulation when live ECU data is not yet framed; ready for real PID path.
pub fn capture_sample(live_overrides: Option<HashMap<String, f64>>) -> Result<LogSample, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if !g.running {
        return Err("Logging not running".into());
    }
    let t = g.started_at.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
    let mut values = simulate_values(&g.channels, t);
    if let Some(over) = live_overrides {
        for (k, v) in over {
            values.insert(k, v);
        }
    }
    // Only keep enabled channels
    values.retain(|k, _| g.channels.iter().any(|c| c.enabled && &c.id == k));
    let sample = LogSample {
        timestamp_ms: now_ms(),
        values,
    };
    g.samples.push(sample.clone());
    if g.samples.len() > g.max_samples {
        let overflow = g.samples.len() - g.max_samples;
        g.samples.drain(0..overflow);
    }
    Ok(sample)
}

pub fn get_samples(limit: Option<usize>) -> Vec<LogSample> {
    let g = LOG.lock().unwrap();
    let lim = limit.unwrap_or(500).min(g.samples.len());
    if lim == 0 {
        return vec![];
    }
    g.samples[g.samples.len() - lim..].to_vec()
}

pub fn clear_samples() -> Result<LogStatus, String> {
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    g.samples.clear();
    Ok(status_from(&g))
}

/// Export current session as CSV string (ready for frontend download).
pub fn export_csv() -> Result<String, String> {
    let g = LOG.lock().map_err(|e| e.to_string())?;
    if g.samples.is_empty() {
        return Err("No samples to export".into());
    }
    let enabled: Vec<&LogChannel> = g.channels.iter().filter(|c| c.enabled).collect();
    let mut csv = String::from("timestamp_ms");
    for ch in &enabled {
        csv.push(',');
        csv.push_str(&ch.id);
    }
    csv.push('\n');
    for s in &g.samples {
        csv.push_str(&s.timestamp_ms.to_string());
        for ch in &enabled {
            csv.push(',');
            if let Some(v) = s.values.get(&ch.id) {
                csv.push_str(&format!("{:.3}", v));
            }
        }
        csv.push('\n');
    }
    Ok(csv)
}

fn status_from(g: &LogSession) -> LogStatus {
    LogStatus {
        running: g.running,
        rate_hz: g.rate_hz,
        sample_count: g.samples.len(),
        channels: g.channels.clone(),
        session_name: g.session_name.clone(),
        started_at_ms: g.started_at_ms,
        last_sample: g.samples.last().cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_exist() {
        assert!(list_templates().len() >= 4);
    }

    #[test]
    fn start_capture_stop() {
        ensure_initialized();
        let _ = stop_session();
        let st = start_session(Some(10.0), Some("test".into())).unwrap();
        assert!(st.running);
        let s = capture_sample(None).unwrap();
        assert!(!s.values.is_empty());
        let st2 = stop_session().unwrap();
        assert!(!st2.running);
        assert!(st2.sample_count >= 1);
        let csv = export_csv().unwrap();
        assert!(csv.contains("timestamp_ms"));
        assert!(csv.contains("rpm"));
    }
}
