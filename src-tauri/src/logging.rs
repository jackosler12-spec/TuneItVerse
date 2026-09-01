//! logging.rs — TuneItVerse data logging engine + CSV import

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn default_channels() -> Vec<LogChannel> {
    vec![
        LogChannel { id: "rpm".into(), name: "Engine RPM".into(), unit: "RPM".into(), pid: 0x000C, enabled: true },
        LogChannel { id: "map".into(), name: "MAP".into(), unit: "kPa".into(), pid: 0x000B, enabled: true },
        LogChannel { id: "ect".into(), name: "ECT".into(), unit: "C".into(), pid: 0x0005, enabled: true },
        LogChannel { id: "tps".into(), name: "TPS".into(), unit: "%".into(), pid: 0x0011, enabled: true },
        LogChannel { id: "iat".into(), name: "IAT".into(), unit: "C".into(), pid: 0x000F, enabled: true },
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
        LogChannel { id: "maf".into(), name: "MAF".into(), unit: "g/s".into(), pid: 0x0010, enabled: false },
        LogChannel { id: "vss".into(), name: "Vehicle Speed".into(), unit: "km/h".into(), pid: 0x000D, enabled: false },
        LogChannel { id: "load".into(), name: "Engine Load".into(), unit: "%".into(), pid: 0x0004, enabled: false },
        LogChannel { id: "o2b1s1".into(), name: "O2 B1S1".into(), unit: "V".into(), pid: 0x0014, enabled: false },
    ]
}

fn templates() -> Vec<LogTemplate> {
    vec![
        LogTemplate { id: "base".into(), name: "Base Street".into(), description: "RPM MAP ECT TPS".into(), pids: vec!["rpm".into(), "map".into(), "ect".into(), "tps".into(), "batt".into()], rate_hz: 10.0 },
        LogTemplate { id: "boost".into(), name: "Boost / Turbo".into(), description: "Boost focused".into(), pids: vec!["rpm".into(), "map".into(), "boost".into(), "tps".into()], rate_hz: 20.0 },
        LogTemplate { id: "diesel".into(), name: "Diesel".into(), description: "EDC style".into(), pids: vec!["rpm".into(), "boost".into(), "rail".into(), "iq".into(), "vgt".into(), "egr".into()], rate_hz: 10.0 },
        LogTemplate { id: "ls1".into(), name: "LS1 / P01".into(), description: "LS1 set".into(), pids: vec!["rpm".into(), "map".into(), "tps".into(), "spark".into(), "stft".into(), "ltft".into()], rate_hz: 20.0 },
        LogTemplate { id: "full".into(), name: "Full".into(), description: "All channels".into(), pids: default_channels().into_iter().map(|c| c.id).collect(), rate_hz: 5.0 },
    ]
}

fn simulate_values(channels: &[LogChannel], t: f64) -> HashMap<String, f64> {
    let mut m = HashMap::new();
    let rpm_base = 1800.0 + 400.0 * (t * 0.7).sin();
    for ch in channels.iter().filter(|c| c.enabled) {
        let v = match ch.id.as_str() {
            "rpm" => rpm_base,
            "map" => 45.0 + 25.0 * (t * 0.5).sin().abs(),
            "ect" => 88.0,
            "tps" => 18.0 + 30.0 * (t * 0.4).sin().abs(),
            "iat" => 32.0,
            "spark" => 18.0,
            "stft" => 0.0,
            "ltft" => 0.0,
            "batt" => 13.9,
            "inj_ms" => 3.2,
            "boost" => 20.0,
            "rail" => 450.0,
            "iq" => 25.0,
            "vgt" => 40.0,
            "egr" => 15.0,
            "maf" => 12.0,
            "vss" => 40.0,
            "load" => 30.0,
            "o2b1s1" => 0.45,
            _ => 0.0,
        };
        m.insert(ch.id.clone(), (v * 100.0).round() / 100.0);
    }
    m
}

pub fn ensure_initialized() {
    let mut g = LOG.lock().unwrap();
    if g.channels.is_empty() {
        g.channels = default_channels();
        g.session_name = format!("log_{}", now_ms());
    }
}

pub fn list_templates() -> Vec<LogTemplate> { templates() }

pub fn get_status() -> LogStatus {
    ensure_initialized();
    let g = LOG.lock().unwrap();
    status_from(&g)
}

pub fn start_session(rate_hz: Option<f64>, session_name: Option<String>) -> Result<LogStatus, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if g.running { return Err("Logging already running. Stop first.".into()); }
    g.rate_hz = rate_hz.unwrap_or(10.0).clamp(1.0, 50.0);
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
    if !g.channels.iter().any(|c| c.enabled) {
        if let Some(c) = g.channels.iter_mut().find(|c| c.id == "rpm") { c.enabled = true; }
    }
    Ok(status_from(&g))
}

pub fn apply_template(template_id: &str) -> Result<LogStatus, String> {
    ensure_initialized();
    let tmpl = templates().into_iter().find(|t| t.id.eq_ignore_ascii_case(template_id)).ok_or_else(|| format!("Unknown template: {}", template_id))?;
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    for ch in g.channels.iter_mut() {
        ch.enabled = tmpl.pids.iter().any(|p| p.eq_ignore_ascii_case(&ch.id));
    }
    g.rate_hz = tmpl.rate_hz;
    Ok(status_from(&g))
}

pub fn capture_sample(live_overrides: Option<HashMap<String, f64>>) -> Result<LogSample, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if !g.running { return Err("Logging not running".into()); }
    let t = g.started_at.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
    let mut values = simulate_values(&g.channels, t);
    if let Some(over) = live_overrides {
        for (k, v) in over { values.insert(k, v); }
    }
    values.retain(|k, _| g.channels.iter().any(|c| c.enabled && &c.id == k));
    let sample = LogSample { timestamp_ms: now_ms(), values };
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
    if lim == 0 { return vec![]; }
    g.samples[g.samples.len() - lim..].to_vec()
}

pub fn clear_samples() -> Result<LogStatus, String> {
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    g.samples.clear();
    Ok(status_from(&g))
}

pub fn export_csv() -> Result<String, String> {
    let g = LOG.lock().map_err(|e| e.to_string())?;
    if g.samples.is_empty() { return Err("No samples to export".into()); }
    let enabled: Vec<&LogChannel> = g.channels.iter().filter(|c| c.enabled).collect();
    let mut csv = String::from("timestamp_ms");
    for ch in &enabled { csv.push(','); csv.push_str(&ch.id); }
    csv.push('\n');
    for s in &g.samples {
        csv.push_str(&s.timestamp_ms.to_string());
        for ch in &enabled {
            csv.push(',');
            if let Some(v) = s.values.get(&ch.id) { csv.push_str(&format!("{:.3}", v)); }
        }
        csv.push('\n');
    }
    Ok(csv)
}

pub fn import_csv(csv: &str) -> Result<LogStatus, String> {
    ensure_initialized();
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or_else(|| "CSV is empty".to_string())?;
    let cols: Vec<String> = header.split(',').map(|s| s.trim().trim_matches('"').to_ascii_lowercase()).collect();
    let ts_idx = cols.iter().position(|c| c == "timestamp_ms" || c == "timestamp" || c == "time_ms");
    let mut imported = Vec::new();
    let mut used_ids: Vec<String> = Vec::new();
    for (row_i, line) in lines.enumerate() {
        let cells: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
        let mut values = HashMap::new();
        for (i, col) in cols.iter().enumerate() {
            if ts_idx == Some(i) || col.is_empty() { continue; }
            if let Some(cell) = cells.get(i) {
                if let Ok(v) = cell.parse::<f64>() {
                    values.insert(col.clone(), v);
                    if !used_ids.iter().any(|id| id == col) { used_ids.push(col.clone()); }
                }
            }
        }
        if values.is_empty() { continue; }
        let timestamp_ms = ts_idx.and_then(|i| cells.get(i)).and_then(|s| s.parse::<u64>().ok()).unwrap_or(now_ms() + row_i as u64);
        imported.push(LogSample { timestamp_ms, values });
    }
    if imported.is_empty() { return Err("No numeric rows found in CSV".into()); }
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    for id in &used_ids {
        if !g.channels.iter().any(|c| c.id == *id) {
            g.channels.push(LogChannel { id: id.clone(), name: id.clone(), unit: "".into(), pid: 0, enabled: true });
        }
    }
    for ch in g.channels.iter_mut() {
        ch.enabled = used_ids.iter().any(|id| id == &ch.id);
    }
    g.samples = imported;
    g.running = false;
    g.session_name = format!("imported_{}", now_ms());
    Ok(status_from(&g))
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
    fn templates_exist() { assert!(list_templates().len() >= 4); }
    #[test]
    fn import_csv_roundtrip() {
        let _ = stop_session();
        let st = import_csv("timestamp_ms,rpm,map\n1,2000,60\n2,2100,62\n").unwrap();
        assert_eq!(st.sample_count, 2);
    }
}
