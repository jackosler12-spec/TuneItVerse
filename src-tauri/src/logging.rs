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
    /// True only for simulated samples from explicit Demo mode. Never set on the live path.
    #[serde(default)]
    pub demo: bool,
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
    /// Demo mode: samples are simulated, not read from an ECU.
    pub demo: bool,
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
    demo: bool,
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
    demo: false,
});

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn default_channels() -> Vec<LogChannel> {
    let mut v = vec![
        LogChannel { id: "rpm".into(), name: "Engine RPM".into(), unit: "RPM".into(), pid: 0x000C, enabled: true },
        LogChannel { id: "map".into(), name: "MAP".into(), unit: "kPa".into(), pid: 0x000B, enabled: true },
        LogChannel { id: "ect".into(), name: "ECT".into(), unit: "C".into(), pid: 0x0005, enabled: true },
        LogChannel { id: "tps".into(), name: "TPS".into(), unit: "%".into(), pid: 0x0011, enabled: true },
        LogChannel { id: "iat".into(), name: "IAT".into(), unit: "C".into(), pid: 0x000F, enabled: true },
        LogChannel { id: "spark".into(), name: "Spark Advance".into(), unit: "deg".into(), pid: 0x000E, enabled: true },
        LogChannel { id: "stft".into(), name: "STFT Bank1".into(), unit: "%".into(), pid: 0x0006, enabled: false },
        LogChannel { id: "ltft".into(), name: "LTFT Bank1".into(), unit: "%".into(), pid: 0x0007, enabled: false },
        // Supply voltage: adapter STVR/ATRV first, Mode 01 PID 0x42 only if the ECU lists it.
        // (The old GM Mode 22 IDs 0x1141/0x125A/0x1171 were never polled and do not apply to
        // other makes, so they are no longer listed as live channels.)
        LogChannel { id: "batt".into(), name: "Battery (adapter STVR/ATRV or PID 0x42)".into(), unit: "V".into(), pid: 0x0042, enabled: true },
        LogChannel { id: "maf".into(), name: "MAF".into(), unit: "g/s".into(), pid: 0x0010, enabled: false },
        LogChannel { id: "vss".into(), name: "Vehicle Speed".into(), unit: "km/h".into(), pid: 0x000D, enabled: false },
        LogChannel { id: "load".into(), name: "Engine Load".into(), unit: "%".into(), pid: 0x0004, enabled: false },
        LogChannel { id: "o2b1s1".into(), name: "O2 B1S1".into(), unit: "V".into(), pid: 0x0014, enabled: false },
        LogChannel { id: "rail".into(), name: "Fuel Rail Gauge Pressure".into(), unit: "kPa".into(), pid: 0x0023, enabled: false },
        LogChannel { id: "dist_mil".into(), name: "Distance with MIL on".into(), unit: "km".into(), pid: 0x0021, enabled: false },
    ];
    v.extend(nissan_raw_channels());
    v
}

/// Optional raw channels, one per Nissan DID the reference Patrol advertises.
/// Off by default. The value is the raw big-endian integer; the meaning is unknown.
pub fn nissan_raw_channels() -> Vec<LogChannel> {
    crate::consult::PATROL_ZD30_SUPPORTED_IDS
        .iter()
        .map(|&d| LogChannel {
            id: format!("nissan_{:04X}", d),
            name: format!("Nissan DID {:04X} (unknown meaning, raw)", d),
            unit: "raw".into(),
            pid: d,
            enabled: false,
        })
        .collect()
}

fn templates() -> Vec<LogTemplate> {
    vec![
        LogTemplate { id: "base".into(), name: "Base Street".into(), description: "RPM MAP ECT TPS".into(), pids: vec!["rpm".into(), "map".into(), "ect".into(), "tps".into(), "batt".into()], rate_hz: 10.0 },
        LogTemplate { id: "boost".into(), name: "Boost / Turbo".into(), description: "MAP-focused street set (Mode 01 only)".into(), pids: vec!["rpm".into(), "map".into(), "tps".into(), "load".into()], rate_hz: 20.0 },
        LogTemplate { id: "diesel".into(), name: "Diesel OBD".into(), description: "Standard SAE Mode 01 PIDs incl. 0x23 fuel rail pressure; only PIDs the ECU reports as supported are polled. No IQ/VGT (not standard PIDs).".into(), pids: vec!["rpm".into(), "map".into(), "ect".into(), "vss".into(), "maf".into(), "load".into(), "rail".into(), "batt".into()], rate_hz: 10.0 },
        LogTemplate { id: "ls1".into(), name: "LS1 / P01".into(), description: "LS1 set".into(), pids: vec!["rpm".into(), "map".into(), "tps".into(), "spark".into(), "stft".into(), "ltft".into()], rate_hz: 20.0 },
        LogTemplate { id: "full".into(), name: "Full".into(), description: "All channels".into(), pids: default_channels().into_iter().map(|c| c.id).filter(|id| !id.starts_with("nissan_")).collect(), rate_hz: 5.0 },
    ]
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

pub const NO_LIVE_CONNECTION: &str = "No live connection: connect an adapter to log real PIDs, or enable Demo mode (simulated data).";

/// Real-path capture. `live` is the PID map read over the active connection
/// (`None` = no connection). Never invents values.
pub fn capture_sample(live: Option<HashMap<String, f64>>) -> Result<LogSample, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if !g.running { return Err("Logging not running".into()); }
    if g.demo { return Err("Demo mode is on. Turn it off to log from the ECU.".into()); }
    let live = live.ok_or_else(|| NO_LIVE_CONNECTION.to_string())?;
    let mut values = HashMap::new();
    for (k, v) in live {
        if g.channels.iter().any(|c| c.enabled && c.id == k) {
            values.insert(k, v);
        }
    }
    if values.is_empty() {
        return Err("Connected, but no enabled PID answered. Nothing recorded.".into());
    }
    let sample = LogSample { timestamp_ms: now_ms(), values, demo: false };
    push_sample(&mut g, sample.clone());
    Ok(sample)
}

fn push_sample(g: &mut LogSession, sample: LogSample) {
    g.samples.push(sample);
    if g.samples.len() > g.max_samples {
        let overflow = g.samples.len() - g.max_samples;
        g.samples.drain(0..overflow);
    }
}

/// Toggle explicit Demo mode. Not allowed mid-session; clears samples so demo
/// and real data never mix.
pub fn set_demo_mode(enabled: bool) -> Result<LogStatus, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if g.running { return Err("Stop logging before changing Demo mode.".into()); }
    if g.demo != enabled { g.samples.clear(); }
    g.demo = enabled;
    Ok(status_from(&g))
}

/// Simulated sample. Only works in Demo mode; every sample is flagged `demo: true`.
pub fn capture_demo_sample() -> Result<LogSample, String> {
    ensure_initialized();
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    if !g.demo { return Err("Demo mode is off. Simulated data is only available in Demo mode.".into()); }
    if !g.running { return Err("Logging not running".into()); }
    let t = g.samples.len() as f64 * 0.1;
    let mut values = HashMap::new();
    for ch in g.channels.iter().filter(|c| c.enabled) {
        let v = match ch.id.as_str() {
            "rpm" => 2000.0 + 800.0 * t.sin(),
            "map" => 60.0 + 20.0 * (t * 0.7).sin(),
            "ect" => 88.0,
            "iat" => 30.0,
            "tps" => 20.0 + 15.0 * (t * 0.5).sin().abs(),
            "batt" => 13.8,
            _ => 0.0,
        };
        values.insert(ch.id.clone(), (v * 10.0).round() / 10.0);
    }
    let sample = LogSample { timestamp_ms: now_ms(), values, demo: true };
    push_sample(&mut g, sample.clone());
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
    let mut csv = String::new();
    if g.samples.iter().any(|s| s.demo) {
        csv.push_str("# DEMO DATA - simulated, not read from an ECU\n");
    }
    csv.push_str("timestamp_ms");
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

/// Split one CSV line on `delim`, honouring double quotes (`""` = literal quote).
fn split_csv_line(line: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == delim {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// Columns that are never imported (location data).
fn is_gps_column(col: &str) -> bool {
    let c = col.trim().to_ascii_lowercase();
    matches!(c.as_str(), "latitude" | "longitude" | "longtitude" | "lat" | "lon" | "lng" | "altitude" | "gps" | "gps_lat" | "gps_lon")
        || c.starts_with("gps ")
}

/// Car Scanner names for standard SAE PIDs → app channel ids.
fn carscanner_channel_id(name: &str) -> String {
    match name.trim() {
        "Engine RPM" => "rpm".into(),
        "Engine coolant temperature" => "ect".into(),
        "Vehicle speed" => "vss".into(),
        "MAF air flow rate" => "maf".into(),
        "Intake manifold absolute pressure" => "map".into(),
        "Fuel rail press." | "Fuel rail pressure" => "rail".into(),
        "Calculated engine load value" | "Calculated engine load" => "load".into(),
        other => {
            let mut id = String::new();
            for ch in other.chars() {
                if ch.is_ascii_alphanumeric() {
                    id.push(ch.to_ascii_lowercase());
                } else if !id.ends_with('_') {
                    id.push('_');
                }
            }
            id.trim_matches('_').to_string()
        }
    }
}

/// Car Scanner "long" export: `SECONDS;PID;VALUE;UNITS;LATITUDE;LONGTITUDE`
/// (sic), one reading per row. Pivoted into one sample per distinct SECONDS
/// value; GPS columns are dropped. Returns (samples, channel ids, units).
fn parse_carscanner_long(lines: &[&str]) -> Result<(Vec<LogSample>, Vec<String>, HashMap<String, String>), String> {
    let header = split_csv_line(lines[0], ';');
    let idx = |name: &str| header.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (ts_i, pid_i, val_i) = (idx("SECONDS").ok_or("no SECONDS column")?, idx("PID").ok_or("no PID column")?, idx("VALUE").ok_or("no VALUE column")?);
    let unit_i = idx("UNITS");
    let mut order: Vec<String> = Vec::new();
    let mut rows: HashMap<String, (f64, HashMap<String, f64>)> = HashMap::new();
    let mut ids: Vec<String> = Vec::new();
    let mut units: HashMap<String, String> = HashMap::new();
    for line in &lines[1..] {
        let cells = split_csv_line(line, ';');
        let (Some(ts), Some(name), Some(val)) = (cells.get(ts_i), cells.get(pid_i), cells.get(val_i)) else { continue };
        let Ok(secs) = ts.parse::<f64>() else { continue };
        let Ok(v) = val.parse::<f64>() else { continue };
        let id = carscanner_channel_id(name);
        if id.is_empty() || is_gps_column(&id) {
            continue;
        }
        if !ids.contains(&id) {
            ids.push(id.clone());
        }
        if let Some(u) = unit_i.and_then(|i| cells.get(i)) {
            units.entry(id.clone()).or_insert_with(|| u.clone());
        }
        let entry = rows.entry(ts.clone()).or_insert_with(|| {
            order.push(ts.clone());
            (secs, HashMap::new())
        });
        entry.1.insert(id, v);
    }
    let samples: Vec<LogSample> = order
        .into_iter()
        .filter_map(|k| rows.remove(&k))
        .map(|(secs, values)| LogSample { timestamp_ms: (secs * 1000.0).round().max(0.0) as u64, values, demo: false })
        .collect();
    Ok((samples, ids, units))
}

pub fn import_csv(csv: &str) -> Result<LogStatus, String> {
    ensure_initialized();
    let lines: Vec<&str> = csv
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}'))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect();
    let header = *lines.first().ok_or_else(|| "CSV is empty".to_string())?;
    let mut units: HashMap<String, String> = HashMap::new();
    let (imported, used_ids) = if header.contains(';') && {
        let h = split_csv_line(header, ';');
        ["SECONDS", "PID", "VALUE"].iter().all(|n| h.iter().any(|c| c.eq_ignore_ascii_case(n)))
    } {
        let (s, ids, u) = parse_carscanner_long(&lines)?;
        units = u;
        (s, ids)
    } else {
        let delim = if header.contains(';') && !header.contains(',') { ';' } else { ',' };
        let cols: Vec<String> = split_csv_line(header, delim).into_iter().map(|s| s.to_ascii_lowercase()).collect();
        let ts_idx = cols.iter().position(|c| c == "timestamp_ms" || c == "timestamp" || c == "time_ms");
        let mut imported = Vec::new();
        let mut used_ids: Vec<String> = Vec::new();
        for (row_i, line) in lines[1..].iter().enumerate() {
            let cells = split_csv_line(line, delim);
            let mut values = HashMap::new();
            for (i, col) in cols.iter().enumerate() {
                if ts_idx == Some(i) || col.is_empty() || is_gps_column(col) { continue; }
                if let Some(cell) = cells.get(i) {
                    if let Ok(v) = cell.parse::<f64>() {
                        values.insert(col.clone(), v);
                        if !used_ids.iter().any(|id| id == col) { used_ids.push(col.clone()); }
                    }
                }
            }
            if values.is_empty() { continue; }
            let timestamp_ms = ts_idx.and_then(|i| cells.get(i)).and_then(|s| s.parse::<u64>().ok()).unwrap_or(now_ms() + row_i as u64);
            imported.push(LogSample { timestamp_ms, values, demo: false });
        }
        (imported, used_ids)
    };
    if imported.is_empty() { return Err("No numeric rows found in CSV".into()); }
    let mut g = LOG.lock().map_err(|e| e.to_string())?;
    for id in &used_ids {
        if !g.channels.iter().any(|c| c.id == *id) {
            let unit = units.get(id).cloned().unwrap_or_default();
            g.channels.push(LogChannel { id: id.clone(), name: id.clone(), unit, pid: 0, enabled: true });
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
        demo: g.demo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    fn reset_for_tests() -> std::sync::MutexGuard<'static, ()> {
        let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut g = LOG.lock().unwrap();
        *g = LogSession {
            running: false,
            rate_hz: 10.0,
            channels: vec![],
            samples: vec![],
            session_name: String::new(),
            started_at: None,
            started_at_ms: None,
            max_samples: 50_000,
            demo: false,
        };
        lock
    }
    #[test]
    fn templates_exist() { assert!(list_templates().len() >= 4); }
    #[test]
    fn diesel_template_has_no_invented_pids() {
        let d = templates().into_iter().find(|t| t.id == "diesel").unwrap();
        // Rail pressure is SAE J1979 PID 0x23 (seen live on the Patrol), so it belongs here.
        assert!(d.pids.iter().any(|p| p == "rail"));
        for banned in ["iq", "vgt", "boost"] {
            assert!(!d.pids.iter().any(|p| p == banned), "diesel template still lists {}", banned);
        }
        // Every template channel is a real channel with a standard PID source.
        for id in &d.pids {
            assert!(id == "batt" || crate::transport::mode01_pid_for(id).is_some(), "{id}");
            assert!(default_channels().iter().any(|c| &c.id == id), "{id}");
        }
    }
    #[test]
    fn no_gm_mode22_ids_in_live_channels() {
        for c in default_channels() {
            assert!(![0x1141u16, 0x125A, 0x1171].contains(&c.pid), "{} uses a GM-only ID", c.id);
        }
    }
    #[test]
    fn nissan_raw_channels_are_labelled_unknown_and_off() {
        let n: Vec<LogChannel> = default_channels().into_iter().filter(|c| c.id.starts_with("nissan_")).collect();
        assert_eq!(n.len(), 32);
        for c in &n {
            assert!(!c.enabled && c.unit == "raw" && c.name.contains("unknown meaning"), "{}", c.name);
            assert_eq!(crate::transport::nissan_raw_channel_did(&c.id), Some(c.pid));
        }
        assert_eq!(crate::transport::nissan_raw_channel_did("nissan_1209"), None, "not in the mask");
    }
    #[test]
    fn import_csv_roundtrip() {
        let _lock = reset_for_tests();
        let st = import_csv("timestamp_ms,rpm,map\n1,2000,60\n2,2100,62\n").unwrap();
        assert_eq!(st.sample_count, 2);
    }
    #[test]
    fn import_carscanner_long_format_pivots_and_drops_gps() {
        let _lock = reset_for_tests();
        // Synthetic rows in Car Scanner's long layout (header copied verbatim,
        // incl. the 'LONGTITUDE' typo); GPS cells are dummies and must be dropped.
        let csv = include_str!("../../reference/fixtures/carscanner_long_sample.csv");
        let st = import_csv(csv).unwrap();
        assert_eq!(st.sample_count, 3);
        let s = get_samples(None);
        assert_eq!(s[0].timestamp_ms, 100_250);
        assert_eq!(s[0].values.get("rpm"), Some(&751.0));
        assert_eq!(s[0].values.get("engine_rpm_x1000"), Some(&0.8));
        assert_eq!(s[1].values.get("rail"), Some(&33180.0));
        assert_eq!(s[2].values.get("ect"), Some(&40.0));
        for smp in &s {
            for k in smp.values.keys() {
                assert!(!k.contains("lat") && !k.contains("long"), "GPS leaked: {k}");
            }
        }
        assert!(!st.channels.iter().any(|c| c.enabled && (c.id.contains("lat") || c.id.contains("long"))));
        assert_eq!(st.channels.iter().find(|c| c.id == "engine_rpm_x1000").map(|c| c.unit.as_str()), Some("rpm"));
    }
    #[test]
    fn import_wide_csv_drops_gps_columns() {
        let _lock = reset_for_tests();
        let st = import_csv("timestamp_ms,rpm,latitude,longitude\n1,800,-27.1,153.2\n").unwrap();
        assert_eq!(st.sample_count, 1);
        assert!(!get_samples(None)[0].values.contains_key("latitude"));
    }
    #[test]
    fn csv_split_handles_quotes() {
        assert_eq!(split_csv_line("\"a;b\";\"c\"\"d\";3", ';'), vec!["a;b", "c\"d", "3"]);
    }
    #[test]
    fn capture_without_live_connection_errors() {
        let _lock = reset_for_tests();
        start_session(Some(10.0), Some("honest".into())).unwrap();
        let e = capture_sample(None).unwrap_err();
        assert!(e.contains("No live connection"));
        assert_eq!(get_samples(None).len(), 0, "nothing recorded");
        let _ = stop_session();
    }
    #[test]
    fn capture_with_no_answers_errors() {
        let _lock = reset_for_tests();
        start_session(Some(10.0), None).unwrap();
        assert!(capture_sample(Some(HashMap::new())).is_err());
        assert_eq!(get_samples(None).len(), 0);
        let _ = stop_session();
    }
    #[test]
    fn demo_only_when_enabled_and_flagged() {
        let _lock = reset_for_tests();
        start_session(Some(10.0), None).unwrap();
        assert!(capture_demo_sample().is_err(), "demo off must refuse");
        assert!(set_demo_mode(true).is_err(), "no toggle mid-session");
        let _ = stop_session();
        assert!(set_demo_mode(true).unwrap().demo);
        start_session(Some(10.0), None).unwrap();
        let s = capture_demo_sample().unwrap();
        assert!(s.demo);
        let mut live = HashMap::new(); live.insert("rpm".into(), 900.0);
        assert!(capture_sample(Some(live)).is_err(), "real path blocked in demo");
        let _ = stop_session();
        assert!(export_csv().unwrap().starts_with("# DEMO DATA"));
        assert!(!set_demo_mode(false).unwrap().demo);
    }
    #[test]
    fn real_samples_not_demo() {
        let _lock = reset_for_tests();
        start_session(Some(10.0), None).unwrap();
        let mut live = HashMap::new(); live.insert("rpm".into(), 900.0);
        assert!(!capture_sample(Some(live)).unwrap().demo);
        let _ = stop_session();
        assert!(!export_csv().unwrap().contains("DEMO"));
    }
    #[test]
    fn capture_keeps_only_live_enabled_keys() {
        let _lock = reset_for_tests();
        start_session(Some(10.0), Some("live".into())).unwrap();
        let mut live = HashMap::new();
        live.insert("rpm".into(), 1850.0);
        live.insert("unknown_pid".into(), 99.0);
        let s = capture_sample(Some(live)).unwrap();
        assert_eq!(s.values.get("rpm"), Some(&1850.0));
        assert!(!s.values.contains_key("ect"));
        assert!(!s.values.contains_key("unknown_pid"));
        let _ = stop_session();
    }
}
