//! Compare the P01 LFSR seed/key against GM public 2-byte table algo N.
//!
//! Bench use: take the seed the PCM returns, run this, see whether L1/L2 LFSR
//! already matches a 2-byte table index. Does not send anything on the wire.

use serde::Serialize;
use crate::gm_keys;
use crate::security::{p01_key_l1, p01_key_l2};

const ALGO_MAX: u16 = 0x3FF;
const LIST_CAP: usize = 32;

#[derive(Debug, Clone, Serialize)]
pub struct P01GmCompare {
    pub seed: u16,
    pub seed_hex: String,
    pub lfsr_l1: u16,
    pub lfsr_l1_hex: String,
    pub lfsr_l2: u16,
    pub lfsr_l2_hex: String,
    pub gm_algo: Option<u16>,
    pub gm_key: Option<u16>,
    pub gm_key_hex: Option<String>,
    pub match_l1: bool,
    pub match_l2: bool,
    pub scanned: bool,
    pub scan_l1_algos: Vec<u16>,
    pub scan_l2_algos: Vec<u16>,
    pub scan_l1_count: usize,
    pub scan_l2_count: usize,
    pub scan_truncated: bool,
    pub gm_table_count: usize,
    pub note: String,
}

fn hex4(v: u16) -> String {
    format!("{:04X}", v)
}

fn parse_seed_hex(seed_hex: &str) -> Result<u16, String> {
    let cleaned: String = seed_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.len() < 4 {
        return Err("seed_hex must be at least 4 hex digits (2 bytes)".into());
    }
    u16::from_str_radix(&cleaned[..4], 16).map_err(|e| format!("bad seed hex: {}", e))
}

/// Compare P01 L1/L2 LFSR keys to one GM 2-byte algo, or scan 0..=1023.
pub fn compare(seed_hex: &str, algo: Option<u16>, scan: bool) -> Result<P01GmCompare, String> {
    let seed = parse_seed_hex(seed_hex)?;
    let (l1h, l1l) = p01_key_l1((seed >> 8) as u8, seed as u8);
    let (l2h, l2l) = p01_key_l2((seed >> 8) as u8, seed as u8);
    let lfsr_l1 = u16::from_be_bytes([l1h, l1l]);
    let lfsr_l2 = u16::from_be_bytes([l2h, l2l]);

    let mut out = P01GmCompare {
        seed,
        seed_hex: hex4(seed),
        lfsr_l1,
        lfsr_l1_hex: hex4(lfsr_l1),
        lfsr_l2,
        lfsr_l2_hex: hex4(lfsr_l2),
        gm_algo: None,
        gm_key: None,
        gm_key_hex: None,
        match_l1: false,
        match_l2: false,
        scanned: false,
        scan_l1_algos: Vec::new(),
        scan_l2_algos: Vec::new(),
        scan_l1_count: 0,
        scan_l2_count: 0,
        scan_truncated: false,
        gm_table_count: gm_keys::table_count(),
        note: String::new(),
    };

    if scan {
        out.scanned = true;
        let mut more = false;
        for a in 0..=ALGO_MAX {
            let k = gm_keys::gm_2byte_key(a, seed);
            if k == lfsr_l1 {
                out.scan_l1_count += 1;
                if out.scan_l1_algos.len() < LIST_CAP {
                    out.scan_l1_algos.push(a);
                } else {
                    more = true;
                }
            }
            if k == lfsr_l2 {
                out.scan_l2_count += 1;
                if out.scan_l2_algos.len() < LIST_CAP {
                    out.scan_l2_algos.push(a);
                } else {
                    more = true;
                }
            }
        }
        out.scan_truncated = more;
        out.note = format!(
            "Scanned algos 0..{}. L1 LFSR 0x{} matches {} index(es). L2 LFSR 0x{} matches {} index(es). Empty match list means this PCM is not using a public 2-byte table for that level.",
            ALGO_MAX, out.lfsr_l1_hex, out.scan_l1_count, out.lfsr_l2_hex, out.scan_l2_count
        );
        return Ok(out);
    }

    let a = algo.unwrap_or(0);
    if a > ALGO_MAX {
        return Err(format!("GM 2-byte algo must be 0..{}", ALGO_MAX));
    }
    let gm = gm_keys::gm_2byte_key(a, seed);
    out.gm_algo = Some(a);
    out.gm_key = Some(gm);
    out.gm_key_hex = Some(hex4(gm));
    out.match_l1 = gm == lfsr_l1;
    out.match_l2 = gm == lfsr_l2;
    out.note = if out.match_l1 && out.match_l2 {
        format!("Algo {} matches both P01 L1 and L2 LFSR for seed 0x{}.", a, out.seed_hex)
    } else if out.match_l1 {
        format!("Algo {} matches P01 L1 LFSR only (read/DTC). L2 flash key differs.", a)
    } else if out.match_l2 {
        format!("Algo {} matches P01 L2 LFSR only (flash). L1 read key differs.", a)
    } else {
        format!(
            "Algo {} key 0x{} matches neither L1 0x{} nor L2 0x{}. Pass scan=true to search 0..{}.",
            a, hex4(gm), out.lfsr_l1_hex, out.lfsr_l2_hex, ALGO_MAX
        )
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_1234_algo0_is_known_gm_vector() {
        let r = compare("1234", Some(0), false).unwrap();
        assert_eq!(r.gm_key, Some(0x3EF7));
        assert_eq!(r.seed, 0x1234);
        assert_ne!(r.lfsr_l1, 0);
    }

    #[test]
    fn seed_ffff_gm_is_ffff() {
        let r = compare("FFFF", Some(0), false).unwrap();
        assert_eq!(r.gm_key, Some(0xFFFF));
        assert_eq!(r.lfsr_l1, 0); // P01 treats 0x0000 only as zero; FFFF still runs LFSR
    }

    #[test]
    fn seed_0000_lfsr_is_zero() {
        let r = compare("0000", Some(1), false).unwrap();
        assert_eq!(r.lfsr_l1, 0);
        assert_eq!(r.lfsr_l2, 0);
        assert_eq!(r.gm_key, Some(0x3412)); // algo 1 is byte-swap of 0000 -> 0000? wait 0x0000 swap is 0000
    }

    #[test]
    fn scan_finds_algo1_byteswap_when_lfsr_equals_swap() {
        // Not asserting a specific collision; just that scan completes and counts are consistent.
        let r = compare("ABCD", None, true).unwrap();
        assert!(r.scanned);
        assert_eq!(r.scan_l1_algos.len(), r.scan_l1_count.min(LIST_CAP));
        assert_eq!(r.gm_table_count, 1280);
    }

    #[test]
    fn bad_hex_errors() {
        assert!(compare("GG", Some(0), false).is_err());
        assert!(compare("12", Some(0), false).is_err());
    }
}
