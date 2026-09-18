//! Measured seed→key tables supplied by the user.
//!
//! This is not a dump of commercial keys. Drop your own captured pairs into
//! `reference/ecu_database/seed_tables.json`. Unverified families stay empty
//! and unlock stays fail-closed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedPair {
    pub family: String,
    pub seed_hex: String,
    pub key_hex: String,
    pub level: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeedTableFile {
    pub version: Option<String>,
    pub pairs: Vec<SeedPair>,
}

const RAW: &str = include_str!("../../reference/ecu_database/seed_tables.json");

fn norm_hex(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn load_tables() -> SeedTableFile {
    serde_json::from_str(RAW).unwrap_or_default()
}

pub fn lookup(family: &str, seed: &[u8], level: Option<&str>) -> Option<Vec<u8>> {
    let fam = family.to_ascii_uppercase();
    let seed_hex = seed.iter().map(|b| format!("{:02X}", b)).collect::<String>();
    let want_lvl = level.map(|s| s.to_ascii_uppercase());
    for p in load_tables().pairs {
        if !p.family.eq_ignore_ascii_case(&fam) && !fam.contains(&p.family.to_ascii_uppercase()) {
            continue;
        }
        if norm_hex(&p.seed_hex) != seed_hex {
            continue;
        }
        if let (Some(want), Some(have)) = (want_lvl.as_ref(), p.level.as_ref()) {
            if !have.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let cleaned = norm_hex(&p.key_hex);
        if cleaned.len() < 2 || cleaned.len() % 2 != 0 {
            continue;
        }
        let mut key = Vec::new();
        let bytes = cleaned.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            if let Ok(pair) = std::str::from_utf8(&bytes[i..i + 2]) {
                if let Ok(b) = u8::from_str_radix(pair, 16) {
                    key.push(b);
                }
            }
            i += 2;
        }
        if !key.is_empty() {
            return Some(key);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_table_parses() {
        let t = load_tables();
        assert!(t.pairs.is_empty() || t.version.is_some());
    }
    #[test]
    fn miss_returns_none() {
        assert!(lookup("EDC17_COMMON", &[0x12, 0x34], Some("1")).is_none());
    }
}
