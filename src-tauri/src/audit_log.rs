//! Append-only JSONL audit log of every flash and backup attempt.
//! Records no VIN, serial or image bytes: only hashes, family, OS id, gates and result.
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static OVERRIDE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub ts_unix_ms: u128,
    pub kind: String, // "backup" | "flash"
    pub family: String,
    pub os_id: Option<u32>,
    pub image_sha256: Option<String>,
    pub backup_sha256: Option<String>,
    #[serde(default)] pub gates_passed: Vec<String>,
    #[serde(default)] pub gate_failed: Option<String>,
    pub result: String, // "ok" | "refused" | "failed"
    pub detail: String,
}

/// Set by the app at startup to <app data dir>.
pub fn set_dir(dir: PathBuf) { if let Ok(mut g) = OVERRIDE_DIR.lock() { *g = Some(dir); } }

pub fn audit_path() -> PathBuf {
    if let Ok(p) = std::env::var("TIV_AUDIT_LOG") { return PathBuf::from(p); }
    if let Some(d) = OVERRIDE_DIR.lock().ok().and_then(|g| g.clone()) { return d.join("audit.jsonl"); }
    std::env::temp_dir().join("tuneitverse_audit.jsonl")
}

pub fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

pub fn append_to(path: &PathBuf, entry: &AuditEntry) -> Result<(), String> {
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    // append(true) only: never truncates or rewrites existing lines.
    let mut f = OpenOptions::new().create(true).append(true).open(path).map_err(|e| format!("audit log open: {}", e))?;
    let mut line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    line.push('\n');
    f.write_all(line.as_bytes()).map_err(|e| format!("audit log write: {}", e))
}

pub fn append(entry: &AuditEntry) -> Result<(), String> { append_to(&audit_path(), entry) }

pub fn read_from(path: &PathBuf, limit: usize) -> Vec<AuditEntry> {
    let t = std::fs::read_to_string(path).unwrap_or_default();
    let mut v: Vec<AuditEntry> = t.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if v.len() > limit { v.drain(..v.len() - limit); }
    v
}

pub fn result_word(success: bool, error: &Option<String>) -> &'static str {
    if success { "ok" } else if error.as_deref().map_or(false, |e| e.contains("Refus") || e.contains("refus") || e.contains("not confirmed") || e.contains("required") || e.contains("not available") || e.contains("No verified")) { "refused" } else { "failed" }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appends_and_reads_back() {
        let p = std::env::temp_dir().join(format!("tiv_audit_{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let mk = |r: &str| AuditEntry { ts_unix_ms: 1, kind: "backup".into(), family: "P01_0411".into(), os_id: Some(12225074),
            image_sha256: Some("abc".into()), backup_sha256: None, gates_passed: vec!["identify".into()], gate_failed: None, result: r.into(), detail: "t".into() };
        append_to(&p, &mk("ok")).unwrap();
        append_to(&p, &mk("refused")).unwrap();
        let v = read_from(&p, 100);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], mk("ok"));
        assert_eq!(read_from(&p, 1)[0].result, "refused");
        let _ = std::fs::remove_file(&p);
    }
    #[test]
    fn result_classification() {
        assert_eq!(result_word(true, &None), "ok");
        assert_eq!(result_word(false, &Some("Backup required before write. Refusing".into())), "refused");
        assert_eq!(result_word(false, &Some("timeout".into())), "failed");
    }
}
