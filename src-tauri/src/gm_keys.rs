//! GM 2-byte seed/key tables from `reference/2byte-keys.txt`.
//!
//! Public Universal Patcher / PCM Hammer style algorithm index 0..=0x3FF.
//! This is not the licensed 5-byte GM library. Unknown opcodes are no-ops.

use std::sync::OnceLock;

const TABLES_TXT: &str = include_str!("../../reference/2byte-keys.txt");

fn parse_tables(txt: &str) -> Vec<[u8; 13]> {
    let mut tables = Vec::new();
    for line in txt.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('/') {
            continue;
        }
        let mut row = [0u8; 13];
        let mut n = 0usize;
        for part in s.split(',') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            let val = if let Some(hex) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
                u8::from_str_radix(hex, 16).ok()
            } else {
                p.parse::<u8>().ok().or_else(|| u8::from_str_radix(p, 16).ok())
            };
            if let Some(b) = val {
                if n < 13 {
                    row[n] = b;
                    n += 1;
                }
            }
        }
        if n >= 13 {
            tables.push(row);
        }
    }
    tables
}

fn tables() -> &'static [[u8; 13]] {
    static TABLES: OnceLock<Vec<[u8; 13]>> = OnceLock::new();
    TABLES.get_or_init(|| parse_tables(TABLES_TXT)).as_slice()
}

pub fn table_count() -> usize {
    tables().len()
}

/// Universal Patcher `GetKey(algo, seed)`.
pub fn gm_2byte_key(algo: u16, seed: u16) -> u16 {
    if seed == 0xFFFF {
        return 0xFFFF;
    }
    if algo > 0x3FF {
        return 0;
    }
    let tables = tables();
    let idx = algo as usize;
    if idx >= tables.len() {
        return 0;
    }
    key_algo(seed, &tables[idx])
}

fn key_algo(seed: u16, row: &[u8; 13]) -> u16 {
    let mut key = seed;
    let mut byte1: usize = 0;
    loop {
        if byte1 + 2 >= row.len() {
            break;
        }
        let op = row[byte1];
        let hi = row[byte1 + 1];
        let lo = row[byte1 + 2];
        match op {
            5 => {
                key = key.rotate_left(8);
            }
            20 => {
                let arg = u16::from_be_bytes([hi, lo]);
                key = key.wrapping_add(arg);
            }
            42 => {
                if hi >= lo {
                    key = !key;
                } else {
                    key = (!key).wrapping_add(1);
                }
            }
            55 | 117 => {
                let arg = u16::from_be_bytes([lo, hi]);
                key = key.wrapping_add(arg);
            }
            76 => {
                let r = (hi % 16) as u32;
                key = key.rotate_left(r);
            }
            82 => {
                let arg = u16::from_be_bytes([lo, hi]);
                key |= arg;
            }
            107 => {
                let r = (lo % 16) as u32;
                key = key.rotate_right(r);
            }
            126 => {
                let swapped = key.rotate_left(8);
                let arg = if hi >= lo {
                    u16::from_be_bytes([hi, lo])
                } else {
                    u16::from_be_bytes([lo, hi])
                };
                key = swapped.wrapping_add(arg);
            }
            152 => {
                let arg = u16::from_be_bytes([hi, lo]);
                key = key.wrapping_sub(arg);
            }
            248 => {
                let arg = u16::from_be_bytes([lo, hi]);
                key = key.wrapping_sub(arg);
            }
            _ => {}
        }
        if byte1 >= 10 {
            break;
        }
        byte1 += 3;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_1280_tables() {
        assert_eq!(table_count(), 1280);
    }

    #[test]
    fn ffff_seed_is_ffff() {
        assert_eq!(gm_2byte_key(0, 0xFFFF), 0xFFFF);
        assert_eq!(gm_2byte_key(1, 0xFFFF), 0xFFFF);
    }

    #[test]
    fn algo_out_of_range_is_zero() {
        assert_eq!(gm_2byte_key(0x400, 0x1234), 0);
    }

    #[test]
    fn algo0_known_vectors() {
        assert_eq!(gm_2byte_key(0, 0x0000), 0x2EE7);
        assert_eq!(gm_2byte_key(0, 0x0001), 0x2EE7);
        assert_eq!(gm_2byte_key(0, 0x1234), 0x3EF7);
        assert_eq!(gm_2byte_key(0, 0xABCD), 0xAFEF);
    }

    #[test]
    fn algo1_is_byte_swap() {
        assert_eq!(gm_2byte_key(1, 0x1234), 0x3412);
        assert_eq!(gm_2byte_key(1, 0xABCD), 0xCDAB);
    }
}
