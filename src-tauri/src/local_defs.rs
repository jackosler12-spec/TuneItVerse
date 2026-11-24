//! Local definitions folder: the user points TuneItVerse at a folder of XDFs
//! they keep on their own disk. Each file's SHA-256 is checked against the
//! committed manifest (`local_defs_manifest.json`: hashes, OS ids, file names,
//! source and licence notes only, no definition content). Files that are not
//! listed, or whose hash does not match, are refused here (the plain
//! "Load XDF" button still exists for ad-hoc files, without these guarantees).
//! Nothing is bundled, copied or uploaded.
//!
//! Table flags come from each table's description:
//! `[confirmed]` / `[unconfirmed]` tags, heuristic candidates, and P04 tables
//! whose row/column storage order was inferred by smoothness. Unconfirmed,
//! heuristic and inferred-order tables are forced read-only (`patch_blocked`).

use crate::xdf::{parse_tunerpro_xdf, TableDef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

pub const MAX_XDF_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DEPTH: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestEntry {
    pub sha256: String,
    pub file_name: String,
    pub size: u64,
    pub family: String,
    pub os_ids: Vec<String>,
    /// "confirmed" (per-table tags apply), "heuristic" (all tables read-only),
    /// or "untagged" (no per-table tags in the file).
    pub tier: String,
    /// XDFTABLE + XDFCONSTANT items in the file.
    pub items: usize,
    pub source: String,
    pub licence: String,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    entries: Vec<ManifestEntry>,
}

pub fn manifest() -> &'static [ManifestEntry] {
    static M: std::sync::OnceLock<Vec<ManifestEntry>> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        serde_json::from_str::<Manifest>(include_str!("local_defs_manifest.json"))
            .map(|m| m.entries)
            .unwrap_or_default()
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct FileStatus {
    /// Path relative to the chosen folder.
    pub rel_path: String,
    pub size: u64,
    pub sha256: Option<String>,
    /// "verified" | "hash_mismatch" | "unlisted" | "too_large" | "unreadable"
    pub status: &'static str,
    pub loadable: bool,
    pub entry: Option<ManifestEntry>,
    pub message: String,
}

fn sha256_hex(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

fn is_xdf(p: &Path) -> bool {
    p.extension().map(|e| e.eq_ignore_ascii_case("xdf")).unwrap_or(false)
}

fn collect(dir: &Path, base: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
        if meta.file_type().is_symlink() { continue; }
        if meta.is_dir() && depth < MAX_DEPTH {
            collect(&p, base, depth + 1, out);
        } else if meta.is_file() && is_xdf(&p) {
            if let Ok(r) = p.strip_prefix(base) { out.push(r.to_path_buf()); }
        }
    }
}

fn classify_file(entries: &[ManifestEntry], rel: &str, bytes: &[u8]) -> (String, &'static str, Option<ManifestEntry>, String) {
    let sha = sha256_hex(bytes);
    if let Some(e) = entries.iter().find(|e| e.sha256.eq_ignore_ascii_case(&sha)) {
        let msg = format!("Verified: {} OS {} ({}). Licence: {}", e.family, e.os_ids.join(","), e.tier, e.licence);
        return (sha, "verified", Some(e.clone()), msg);
    }
    let name = Path::new(rel).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if let Some(e) = entries.iter().find(|e| e.file_name == name) {
        let msg = format!("Refused: SHA-256 does not match the manifest entry for '{}' (edited, corrupted or a different version).", e.file_name);
        return (sha, "hash_mismatch", None, msg);
    }
    (sha, "unlisted", None, "Not in the manifest: refused here. Use Load XDF if you trust this file.".into())
}

pub fn scan_with(entries: &[ManifestEntry], folder: &Path) -> Result<Vec<FileStatus>, String> {
    if !folder.is_dir() {
        return Err(format!("Not a folder: {}", folder.display()));
    }
    let mut rels = vec![];
    collect(folder, folder, 0, &mut rels);
    let mut out = vec![];
    for rel in rels {
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let full = folder.join(&rel);
        let size = std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0);
        if size > MAX_XDF_BYTES {
            out.push(FileStatus { rel_path: rel_s, size, sha256: None, status: "too_large", loadable: false, entry: None, message: format!("Larger than {} MB: skipped.", MAX_XDF_BYTES >> 20) });
            continue;
        }
        match std::fs::read(&full) {
            Ok(b) => {
                let (sha, status, entry, message) = classify_file(entries, &rel_s, &b);
                out.push(FileStatus { rel_path: rel_s, size, sha256: Some(sha), status, loadable: status == "verified", entry, message });
            }
            Err(e) => out.push(FileStatus { rel_path: rel_s, size, sha256: None, status: "unreadable", loadable: false, entry: None, message: e.to_string() }),
        }
    }
    Ok(out)
}

/// A relative path inside the chosen folder: no `..`, no root, `.xdf` only.
fn safe_rel(rel: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(rel.replace('\\', "/"));
    if rel.trim().is_empty() || !is_xdf(&p) || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("Refused path '{}': must be a .xdf file inside the chosen folder.", rel));
    }
    Ok(p)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TableFlag {
    pub id: String,
    pub name: String,
    /// "confirmed" | "unconfirmed" | "untagged"
    pub confidence: &'static str,
    pub read_only: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct FlagCounts {
    pub confirmed: usize,
    pub unconfirmed: usize,
    pub untagged: usize,
    pub read_only: usize,
    pub inferred_order: usize,
}

pub const RO_HEURISTIC: &str = "Heuristic candidate from a scan: purpose unknown. Read-only.";
pub const RO_UNCONFIRMED: &str = "Tagged [unconfirmed] in its definition. Read-only until confirmed.";
pub const RO_INFERRED_ORDER: &str = "Row/column storage order was inferred by smoothness, not taken from a source. Read-only until verified on a real image.";

fn has_inferred_order(desc_lc: &str) -> bool {
    desc_lc.contains("smoothness") && desc_lc.contains("major")
}

/// Apply confidence flags and read-only locks in place.
pub fn classify_tables(tables: &mut [TableDef], tier: &str) -> (Vec<TableFlag>, FlagCounts) {
    let mut flags = Vec::with_capacity(tables.len());
    let mut n = FlagCounts::default();
    for t in tables.iter_mut() {
        let d = t.description.to_ascii_lowercase();
        let heuristic = tier == "heuristic" || t.name.to_ascii_uppercase().starts_with("UNCONFIRMED") || d.contains("heuristic candidate");
        let confidence = if heuristic || d.contains("[unconfirmed]") {
            "unconfirmed"
        } else if d.contains("[confirmed]") {
            "confirmed"
        } else {
            "untagged"
        };
        let mut reasons: Vec<&str> = vec![];
        if heuristic { reasons.push(RO_HEURISTIC); } else if confidence == "unconfirmed" { reasons.push(RO_UNCONFIRMED); }
        if has_inferred_order(&d) { reasons.push(RO_INFERRED_ORDER); n.inferred_order += 1; }
        if !reasons.is_empty() {
            let mut r = reasons.join(" ");
            if let Some(prev) = &t.patch_blocked { r = format!("{} {}", prev, r); }
            t.patch_blocked = Some(r);
        }
        match confidence { "confirmed" => n.confirmed += 1, "unconfirmed" => n.unconfirmed += 1, _ => n.untagged += 1 }
        if t.patch_blocked.is_some() { n.read_only += 1; }
        flags.push(TableFlag { id: t.id.clone(), name: t.name.clone(), confidence, read_only: t.patch_blocked.clone() });
    }
    (flags, n)
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedDefs {
    pub rel_path: String,
    pub entry: ManifestEntry,
    pub tables: Vec<TableDef>,
    pub skipped: Vec<String>,
    pub flags: Vec<TableFlag>,
    pub counts: FlagCounts,
}

pub fn load_with(entries: &[ManifestEntry], folder: &Path, rel: &str) -> Result<LoadedDefs, String> {
    let relp = safe_rel(rel)?;
    let full = folder.join(&relp);
    let meta = std::fs::symlink_metadata(&full).map_err(|e| format!("{}: {}", rel, e))?;
    if !meta.is_file() || meta.file_type().is_symlink() { return Err(format!("Refused '{}': not a regular file.", rel)); }
    if meta.len() > MAX_XDF_BYTES { return Err(format!("Refused '{}': larger than {} MB.", rel, MAX_XDF_BYTES >> 20)); }
    let bytes = std::fs::read(&full).map_err(|e| format!("{}: {}", rel, e))?;
    // Re-hash at load time: the file may have changed since the scan.
    let (_, status, entry, msg) = classify_file(entries, rel, &bytes);
    let entry = match (status, entry) { ("verified", Some(e)) => e, _ => return Err(msg) };
    let text = String::from_utf8(bytes).map_err(|_| format!("Refused '{}': not a UTF-8 text XDF.", rel))?;
    let parsed = parse_tunerpro_xdf(&text)?;
    let mut tables = parsed.tables;
    let (flags, counts) = classify_tables(&mut tables, &entry.tier);
    Ok(LoadedDefs { rel_path: rel.replace('\\', "/"), entry, tables, skipped: parsed.skipped, flags, counts })
}

// ── Tauri commands (offline) ──────────────────────────────────────────────

#[tauri::command]
pub fn local_defs_manifest() -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({
        "entries": manifest(),
        "note": "Hashes and metadata only. Definitions are never bundled; load them from your own folder.",
    })).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn local_defs_scan(folder: String) -> Result<String, String> {
    let files = scan_with(manifest(), Path::new(&folder))?;
    let verified = files.iter().filter(|f| f.loadable).count();
    serde_json::to_string(&serde_json::json!({ "folder": folder, "files": files, "verified": verified, "manifest_entries": manifest().len() }))
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn local_defs_load(folder: String, rel_path: String) -> Result<LoadedDefs, String> {
    load_with(manifest(), Path::new(&folder), &rel_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HKAASE_12225074: &str = "bc8f40766ed364f8c4d4ecc82deabc909f8e50cbd1ad56265fec30fb27de681d";

    fn xdf(tables: &[(&str, &str)]) -> String {
        let mut s = String::from("<?xml version=\"1.0\"?>\n<XDFFORMAT version=\"1.60\"><XDFHEADER><BASEOFFSET offset=\"0\" subtract=\"0\" /><DEFAULTS datasizeinbits=\"8\" signed=\"0\" lsbfirst=\"0\" float=\"0\" /></XDFHEADER>\n");
        for (i, (title, desc)) in tables.iter().enumerate() {
            s += &format!("<XDFTABLE uniqueid=\"0x{i:X}\"><title>{title}</title><description>{desc}</description><XDFAXIS id=\"x\"><indexcount>2</indexcount></XDFAXIS><XDFAXIS id=\"y\"><indexcount>2</indexcount></XDFAXIS><XDFAXIS id=\"z\"><EMBEDDEDDATA mmedaddress=\"0x{:X}\" mmedelementsizebits=\"8\" mmedrowcount=\"2\" mmedcolcount=\"2\" /><MATH equation=\"X\" /></XDFAXIS></XDFTABLE>\n", 0x10 * (i + 1));
        }
        s + "</XDFFORMAT>\n"
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tiv-localdefs-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        d
    }

    fn entry_for(bytes: &[u8], name: &str, tier: &str) -> ManifestEntry {
        ManifestEntry { sha256: sha256_hex(bytes), file_name: name.into(), size: bytes.len() as u64, family: "TEST".into(), os_ids: vec!["1".into()],
            tier: tier.into(), items: 3, source: "synthetic".into(), licence: "test".into(), notes: String::new() }
    }

    #[test]
    fn committed_manifest_is_hashes_and_metadata_only() {
        let m = manifest();
        assert!(m.len() >= 25, "manifest failed to parse or is short: {}", m.len());
        assert!(m.iter().any(|e| e.sha256 == HKAASE_12225074 && e.os_ids == ["12225074"]));
        let mut seen = std::collections::HashSet::new();
        for e in m {
            assert!(e.sha256.len() == 64 && e.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", e.file_name);
            assert!(seen.insert(e.sha256.clone()), "duplicate {}", e.sha256);
            assert!(!e.licence.trim().is_empty() && !e.source.trim().is_empty(), "{}", e.file_name);
            assert!(["confirmed", "heuristic", "untagged"].contains(&e.tier.as_str()), "{}", e.tier);
            assert!(e.file_name.to_ascii_lowercase().ends_with(".xdf"));
        }
        // No definition content: the manifest is tiny compared with the XDFs it describes.
        assert!(include_str!("local_defs_manifest.json").len() < 64 * 1024);
        assert!(!include_str!("local_defs_manifest.json").contains("<XDF"));
    }

    #[test]
    fn flags_and_read_only_locks() {
        let text = xdf(&[
            ("Spark A", "Main spark.\n[confirmed] source: test"),
            ("VE B", "[column-major chosen (smoothness 10 vs row-major 20)]\n[confirmed] source: test"),
            ("Fuel C", "[unconfirmed] source: test"),
            ("UNCONFIRMED 2D 2x2 @0x40", "Heuristic candidate: purpose unknown.\n[confirmed] source: heuristic scan"),
            ("Plain D", "No tags at all"),
        ]);
        let mut t = parse_tunerpro_xdf(&text).unwrap().tables;
        assert_eq!(t.len(), 5);
        let (f, n) = classify_tables(&mut t, "confirmed");
        assert_eq!(f[0].confidence, "confirmed");
        assert!(f[0].read_only.is_none());
        assert_eq!(f[1].confidence, "confirmed");
        assert!(f[1].read_only.as_deref().unwrap().contains("inferred"));
        assert_eq!(f[2].confidence, "unconfirmed");
        assert!(f[2].read_only.is_some());
        assert_eq!(f[3].confidence, "unconfirmed", "heuristic must never count as confirmed");
        assert!(f[3].read_only.as_deref().unwrap().contains("Heuristic"));
        assert_eq!(f[4].confidence, "untagged");
        assert_eq!((n.confirmed, n.unconfirmed, n.untagged, n.read_only, n.inferred_order), (2, 2, 1, 3, 1));
        // The read-only reason travels on the TableDef, so patch_table_into_bin refuses it.
        assert!(t[1].patch_blocked.is_some() && t[0].patch_blocked.is_none());
        // Heuristic tier locks every table.
        let mut t2 = parse_tunerpro_xdf(&text).unwrap().tables;
        let (_, n2) = classify_tables(&mut t2, "heuristic");
        assert_eq!((n2.unconfirmed, n2.read_only), (5, 5));
    }

    #[test]
    fn scan_verifies_hash_and_refuses_mismatch_and_unlisted() {
        let d = tmpdir("scan");
        let good = xdf(&[("A", "[confirmed] x"), ("B", "x"), ("C", "x")]);
        std::fs::write(d.join("sub/good.xdf"), &good).unwrap();
        std::fs::write(d.join("edited.xdf"), xdf(&[("A", "changed")])).unwrap();
        std::fs::write(d.join("other.xdf"), xdf(&[("Z", "z")])).unwrap();
        std::fs::write(d.join("notes.txt"), "ignored").unwrap();
        let entries = vec![entry_for(good.as_bytes(), "good.xdf", "confirmed"), ManifestEntry { sha256: "0".repeat(64), ..entry_for(b"x", "edited.xdf", "confirmed") }];
        let s = scan_with(&entries, &d).unwrap();
        let get = |p: &str| s.iter().find(|f| f.rel_path == p).unwrap();
        assert_eq!(s.len(), 3, "only .xdf files are listed");
        assert_eq!(get("sub/good.xdf").status, "verified");
        assert!(get("sub/good.xdf").loadable);
        assert_eq!(get("edited.xdf").status, "hash_mismatch");
        assert_eq!(get("other.xdf").status, "unlisted");
        let l = load_with(&entries, &d, "sub/good.xdf").unwrap();
        assert_eq!(l.tables.len(), 3);
        assert_eq!(l.counts.confirmed, 1);
        assert!(load_with(&entries, &d, "edited.xdf").unwrap_err().contains("does not match"));
        assert!(load_with(&entries, &d, "other.xdf").unwrap_err().contains("Not in the manifest"));
        // Tampering after the scan is caught at load time.
        std::fs::write(d.join("sub/good.xdf"), good.replace("[confirmed] x", "[confirmed] y")).unwrap();
        assert!(load_with(&entries, &d, "sub/good.xdf").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn path_escape_refused() {
        let d = tmpdir("paths");
        for bad in ["../x.xdf", "/etc/x.xdf", "sub/../../x.xdf", "", "a.bin", "C:\\x.xdf"] {
            assert!(load_with(manifest(), &d, bad).is_err(), "{bad}");
        }
        assert!(scan_with(manifest(), &d.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Opt-in: TIV_LOCAL_DEFS_DIR = a folder holding the real XDFs (never
    /// committed). Every verified file must load and match its manifest count.
    #[test]
    fn real_defs_folder_when_present() {
        let Ok(dir) = std::env::var("TIV_LOCAL_DEFS_DIR") else { return };
        let s = scan_with(manifest(), Path::new(&dir)).unwrap();
        for f in s.iter().filter(|f| f.loadable) {
            let l = load_with(manifest(), Path::new(&dir), &f.rel_path).unwrap();
            assert_eq!(l.tables.len() + l.skipped.iter().filter(|s| !s.contains("axis not read")).count(), l.entry.items, "{}", f.rel_path);
            if l.entry.tier == "heuristic" { assert_eq!(l.counts.read_only, l.tables.len()); }
            eprintln!("{:<60} tables={} skipped={} conf={} unconf={} untagged={} ro={} inferred={}", f.rel_path, l.tables.len(), l.skipped.len(),
                l.counts.confirmed, l.counts.unconfirmed, l.counts.untagged, l.counts.read_only, l.counts.inferred_order);
        }
        eprintln!("verified {}/{} files", s.iter().filter(|f| f.loadable).count(), s.len());
    }
}
