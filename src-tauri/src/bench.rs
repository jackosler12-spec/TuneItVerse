//! Bench session planner.
//!
//! Checklists for OBD vs bench work on a personal dump. This does not send
//! programming frames, bootloader payloads, or security-access keys.

use serde_json::{json, Value};

fn steps(family: &str, write_live: bool) -> Vec<&'static str> {
    let mut out = vec![
        "Confirm the dump is from a controller you own.",
        "Use a current-limited 13.8 V supply on the bench. Do not program from a weak battery.",
        "Take a full image backup before any edit. Mode 22 samples are not a backup.",
        "Identify OS / part string and match it to the catalog before checksum correction.",
        "Keep the adapter session read-only until voltage, backup, and family gates pass.",
    ];
    if write_live {
        out.push("This family advertises a live write path. The planner still does not start it.");
    } else {
        out.push("Write stays blocked for this family until a measured personal routine exists.");
    }
    let _ = family;
    out
}

fn bench_note(family: &str) -> &'static str {
    match family {
        "P01_0411" => "P01 is a VPW 0411 PCM. Bench work is a powered harness plus a VPW or J2534 interface. Kernel-assisted readback is the full-image path; this planner does not upload a kernel.",
        "EDC16C41" => "EDC16C41 Patrol work is CAN/KWP. Bench mode is a powered ECU and a CAN adapter. BDM pinouts stay user-supplied. This planner does not generate a bootloader sequence.",
        "GM_P59" => "P59 can be identified. Checksum correction and write stay blocked until measured correction words from your dump exist.",
        "HONDA_KEIHIN" => "Honda Keihin stays identify-only. Do not reuse the P01 additive corrector.",
        "TRANSTRON_4HK1" => "Transtron 4HK1 is catalog-only until a personal dump and protocol notes land.",
        _ => "Catalog family. Use OBD for identify/DTC. Bench flash is not offered here.",
    }
}

pub fn plan_for(family: &str) -> Value {
    let key = family.trim();
    let known = crate::ecu_database::get_ecu_by_family(key);
    let family_id = known.as_ref().map(|e| e.ecu_family.clone()).unwrap_or_else(|| key.to_string());
    let write_live = crate::ecu_database::write_path_live(&family_id);
    json!({
        "family": family_id,
        "known": known.is_some(),
        "display_name": known.as_ref().map(|e| e.display_name.clone()),
        "protocol": known.as_ref().map(|e| e.protocol.clone()),
        "bin_size_bytes": known.as_ref().map(|e| e.bin_size_bytes),
        "mode": "plan-only",
        "write_allowed": false,
        "catalog_write_path": write_live,
        "adapters": ["OBDLink MX+", "ELM327-class", "J2534 pass-thru", "FTDI USB-serial"],
        "bdm_jtag": "Foundation only. Add your own pinout notes. No probe script is shipped.",
        "steps": steps(&family_id, write_live),
        "notes": bench_note(&family_id),
    })
}

#[tauri::command]
pub fn bench_session_plan(family: Option<String>) -> Result<String, String> {
    let fam = family.unwrap_or_else(|| "P01_0411".into());
    serde_json::to_string(&plan_for(&fam)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_never_enables_write() {
        for fam in ["P01_0411", "EDC16C41", "GM_P59", "HONDA_KEIHIN", "nope"] {
            let plan = plan_for(fam);
            assert_eq!(plan["write_allowed"], false);
            assert!(plan["steps"].as_array().unwrap().len() >= 5);
        }
    }
}
