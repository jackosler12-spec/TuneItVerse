//! GM OS ID -> PCM hardware type, from PcmHammer `PcmInfo.cs` (public source,
//! pinned commit in `reference/ecu_database/gm_osid_map.json`).
//!
//! Used for two things only:
//! 1. Identification: report what PcmHammer calls the hardware for a binary OS ID.
//! 2. Fail-closed: refuse the P01 checksum corrector when the image's own OS ID
//!    belongs to a different GM PCM (P59, P10, P12, ...), where the P01 segment
//!    layout is not verified.
use std::collections::HashMap;
use std::sync::OnceLock;

const MAP_JSON: &str = include_str!("../../reference/ecu_database/gm_osid_map.json");

fn map() -> &'static HashMap<u32, String> {
    static M: OnceLock<HashMap<u32, String>> = OnceLock::new();
    M.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(MAP_JSON).unwrap_or_default();
        v.get("osids")
            .and_then(|o| o.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, t)| Some((k.parse::<u32>().ok()?, t.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Source URL of the map (pinned commit).
pub fn source() -> String {
    serde_json::from_str::<serde_json::Value>(MAP_JSON)
        .ok()
        .and_then(|v| v.get("source").and_then(|s| s.as_str()).map(str::to_string))
        .unwrap_or_default()
}

pub fn len() -> usize {
    map().len()
}

/// PcmHammer hardware type for an OS ID ("P01", "P59", "P10", "P12", "Undefined", ...).
pub fn pcm_type(osid: u32) -> Option<&'static str> {
    map().get(&osid).map(String::as_str)
}

/// ECU database family for a PcmHammer type, when this app has an entry for it.
pub fn family_for_type(t: &str) -> Option<&'static str> {
    match t {
        "P01" => Some("P01_0411"),
        "P59" => Some("GM_P59"),
        "P10" => Some("GM_P10"),
        "P12" | "P12b" => Some("GM_P12"),
        _ => None,
    }
}

/// Binary OS ID read from a 512 KB image (0x504, only when the OS segment
/// checksum validates) whose PcmHammer type is a known non-P01 PCM.
pub fn non_p01_binary_os(data: &[u8]) -> Option<(u32, &'static str)> {
    let os = crate::checksum::p01_binary_os_id(data)?;
    match pcm_type(os) {
        Some(t) if t != "P01" && t != "Undefined" => Some((os, t)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_loads_and_has_pinned_source() {
        assert!(len() > 2000, "map has {} entries", len());
        assert!(source().contains("PcmHammer/PcmHammer/blob/033cb039"));
    }

    #[test]
    fn known_ids_from_pcminfo() {
        // PcmInfo.cs: "case 12225074: // main 2003/2004 OS" -> P01
        assert_eq!(pcm_type(12225074), Some("P01"));
        assert_eq!(pcm_type(12593358), Some("P01"));
        // PcmInfo.cs: "case 12590777:" -> P59 (P59a Hybrid)
        assert_eq!(pcm_type(12590777), Some("P59"));
        // P10 Service No 12576463 block
        assert_eq!(pcm_type(12213305), Some("P10"));
        // P12 2m block
        assert_eq!(pcm_type(12613422), Some("P12b"));
        assert_eq!(pcm_type(1), None);
    }

    #[test]
    fn family_mapping_is_explicit() {
        assert_eq!(family_for_type("P01"), Some("P01_0411"));
        assert_eq!(family_for_type("P12b"), Some("GM_P12"));
        assert_eq!(family_for_type("E38"), None);
    }

    #[test]
    fn real_p01_dump_is_p01_when_present() {
        // Private dump only via env var; never committed.
        let Ok(path) = std::env::var("TIV_P01_BIN") else { return };
        let Ok(data) = std::fs::read(path) else { return };
        assert_eq!(non_p01_binary_os(&data), None);
        let os = crate::checksum::p01_binary_os_id(&data).unwrap();
        assert_eq!(pcm_type(os), Some("P01"));
    }

    #[test]
    fn reference_p01_image_is_p01() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../reference/LS1 PCM P01 12225074.bin")).unwrap();
        let os = crate::checksum::p01_binary_os_id(&data).unwrap();
        assert_eq!(os, 12225074);
        assert_eq!(pcm_type(os), Some("P01"));
        assert!(non_p01_binary_os(&data).is_none());
    }
}
