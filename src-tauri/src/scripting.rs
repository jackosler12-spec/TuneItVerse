//! Embedded bench script runtime — deterministic BIN/log ops, no arbitrary code.
//!
//! Script language is line-oriented:
//!   identify
//!   checksum
//!   correct
//!   compare
//!   seedkey FAMILY SEEDHEX LEVEL
//!   poke OFFSET HEXBYTES
//!   tables
//!   maplog
//!   help
//!
//! `#` starts a comment. Empty lines are ignored. Commands that mutate the
//! working image return the new bytes to the UI so Save BIN stays honest.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Default)]
struct ScriptState {
    bin: Vec<u8>,
    compare: Vec<u8>,
}

fn hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.is_empty() || cleaned.len() % 2 != 0 {
        return Err("hex must be even-length".into());
    }
    let bytes = cleaned.as_bytes();
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).map_err(|e| e.to_string())?;
        out.push(u8::from_str_radix(pair, 16).map_err(|e| format!("bad hex: {}", e))?);
        i += 2;
    }
    Ok(out)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

fn parse_offset(tok: &str) -> Result<usize, String> {
    let t = tok.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        usize::from_str_radix(hex, 16).map_err(|e| format!("bad offset: {}", e))
    } else {
        t.parse::<usize>().map_err(|e| format!("bad offset: {}", e))
    }
}

fn help_text() -> Value {
    json!({
        "language": "TuneItVerse bench script v3.20",
        "commands": [
            "identify                 — family / OS / size / correction_safe",
            "checksum                 — validate known families (report-only for Honda/P59)",
            "correct                  — apply measured corrector; fail-closed otherwise",
            "compare                  — diff working BIN vs compare image (set via UI)",
            "seedkey FAMILY HEX LVL   — offline seed→key (verified algos only)",
            "poke OFFSET HEX          — write bytes at offset (0x4000 AA BB)",
            "tables                   — locate TableSeek / catalog maps",
            "maplog                   — occupancy heatmap from imported / live log",
            "help                     — this list"
        ],
        "notes": "No eval, no shell, no invented seed tables. Personal dumps only."
    })
}

fn run_line(state: &mut ScriptState, line: &str) -> Result<Value, String> {
    let raw = line.trim();
    if raw.is_empty() || raw.starts_with('#') {
        return Ok(json!({"ok": true, "skip": true}));
    }
    let mut parts = raw.split_whitespace();
    let cmd = parts.next().unwrap_or("").to_ascii_lowercase();
    match cmd.as_str() {
        "help" | "?" => Ok(help_text()),
        "identify" => {
            if state.bin.is_empty() {
                return Err("No working BIN. Load a dump first.".into());
            }
            Ok(crate::v29_tools::identify_bin(&state.bin))
        }
        "checksum" | "validate" => {
            if state.bin.is_empty() {
                return Err("No working BIN.".into());
            }
            let report = crate::checksum::validate_checksums(&state.bin)?;
            Ok(serde_json::to_value(report).unwrap_or(json!({})))
        }
        "correct" => {
            if state.bin.is_empty() {
                return Err("No working BIN.".into());
            }
            let corrected = crate::checksum::correct_checksums(&state.bin)?;
            state.bin = corrected.data.clone();
            Ok(json!({
                "ok": true,
                "mutated": true,
                "bytes": corrected.data.len(),
                "sha256": sha256_hex(&corrected.data),
                "report": corrected.report
            }))
        }
        "compare" | "diff" => {
            if state.bin.is_empty() || state.compare.is_empty() {
                return Err("Need a working BIN and a compare BIN.".into());
            }
            Ok(crate::v29_tools::compare_bins(&state.bin, &state.compare))
        }
        "seedkey" => {
            let family = parts.next().unwrap_or("P01_0411").to_string();
            let seed = parts.next().unwrap_or("");
            let level = parts.next().unwrap_or("1").to_string();
            crate::compute_seed_key(seed.to_string(), Some(family), Some(level))
                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        }
        "poke" => {
            if state.bin.is_empty() {
                return Err("No working BIN.".into());
            }
            let off = parse_offset(parts.next().ok_or("poke OFFSET HEX")?)?;
            let rest: String = parts.collect::<Vec<_>>().join("");
            let bytes = hex_bytes(&rest)?;
            if off + bytes.len() > state.bin.len() {
                return Err(format!("poke past end: {}+{} > {}", off, bytes.len(), state.bin.len()));
            }
            state.bin[off..off + bytes.len()].copy_from_slice(&bytes);
            Ok(json!({
                "ok": true,
                "mutated": true,
                "offset": off,
                "wrote": bytes.len(),
                "sha256": sha256_hex(&state.bin)
            }))
        }
        "tables" => {
            if state.bin.is_empty() {
                return Err("No working BIN.".into());
            }
            let r = crate::ecu_database::tables_for_bin(&state.bin);
            Ok(json!({
                "pack": r.pack,
                "located": r.located,
                "missing": r.missing,
                "note": r.note,
                "first_ids": r.tables.iter().take(12).map(|t| t.id.clone()).collect::<Vec<_>>()
            }))
        }
        "maplog" | "map_from_log" => crate::v29_tools::analyze_log(),
        other => Err(format!("unknown command '{}'. Type help.", other)),
    }
}

pub fn run_script(source: &str, working_bin: Option<Vec<u8>>, compare_bin: Option<Vec<u8>>) -> Value {
    let mut state = ScriptState {
        bin: working_bin.unwrap_or_default(),
        compare: compare_bin.unwrap_or_default(),
    };
    let mut steps = Vec::new();
    let mut mutated = false;
    let mut error: Option<String> = None;
    for (i, line) in source.lines().enumerate() {
        match run_line(&mut state, line) {
            Ok(v) => {
                if v.get("mutated").and_then(|x| x.as_bool()).unwrap_or(false) {
                    mutated = true;
                }
                if v.get("skip").and_then(|x| x.as_bool()).unwrap_or(false) {
                    continue;
                }
                steps.push(json!({"line": i + 1, "ok": true, "result": v}));
            }
            Err(e) => {
                error = Some(e.clone());
                steps.push(json!({"line": i + 1, "ok": false, "error": e}));
                break;
            }
        }
    }
    json!({
        "ok": error.is_none(),
        "error": error,
        "mutated": mutated,
        "bin_len": state.bin.len(),
        "sha256": if state.bin.is_empty() { Value::Null } else { json!(sha256_hex(&state.bin)) },
        "steps": steps,
        "bin": if mutated { json!(state.bin) } else { Value::Null }
    })
}

#[tauri::command]
pub fn run_bench_script(
    source: String,
    working_bin: Option<Vec<u8>>,
    compare_bin: Option<Vec<u8>>,
) -> Result<String, String> {
    Ok(run_script(&source, working_bin, compare_bin).to_string())
}

#[tauri::command]
pub fn list_script_helpers() -> Result<String, String> {
    Ok(json!([
        {
            "id": "identify",
            "name": "Identify dump",
            "description": "In-app: identify. CLI still works.",
            "command": "identify",
            "cli": "python3 python/ecu_scripting.py identify path/to/dump.bin"
        },
        {
            "id": "checksum",
            "name": "Checksum report",
            "description": "Validate known families. Honda / P59 stay report-only.",
            "command": "checksum",
            "cli": "python3 python/ecu_scripting.py checksum path/to/dump.bin"
        },
        {
            "id": "seedkey",
            "name": "Seed/key bench",
            "description": "Verified algos only (P01 LFSR, EDC16C41 4-byte).",
            "command": "seedkey P01_0411 1234 1",
            "cli": "python3 python/ecu_scripting.py seedkey P01_0411 1234 1"
        },
        {
            "id": "diff",
            "name": "BIN diff",
            "description": "Load a compare BIN on Maps, then run compare.",
            "command": "compare",
            "cli": "python3 python/ecu_scripting.py diff stock.bin tuned.bin"
        },
        {
            "id": "maplog",
            "name": "Map from log",
            "description": "Occupancy heatmap from the current log buffer.",
            "command": "maplog",
            "cli": null
        }
    ])
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_runs() {
        let v = run_script("help\n", None, None);
        assert_eq!(v["ok"], true);
        assert_eq!(v["steps"][0]["ok"], true);
    }

    #[test]
    fn poke_mutates() {
        let img = vec![0u8; 8];
        let v = run_script("poke 2 AABB\n", Some(img), None);
        assert_eq!(v["ok"], true);
        assert_eq!(v["mutated"], true);
        let bin = v["bin"].as_array().unwrap();
        assert_eq!(bin[2], 0xAA);
        assert_eq!(bin[3], 0xBB);
    }

    #[test]
    fn unknown_stops() {
        let v = run_script("nope\nidentify\n", Some(vec![0u8; 4]), None);
        assert_eq!(v["ok"], false);
        assert_eq!(v["steps"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn identify_blank_512k_is_collision() {
        let v = run_script("identify\n", Some(vec![0u8; 524288]), None);
        assert_eq!(v["ok"], true);
        let ident = &v["steps"][0]["result"];
        assert_eq!(ident["size_collision"], true);
        assert!(ident["family"].is_null());
    }
}
