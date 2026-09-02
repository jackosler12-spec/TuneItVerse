//! BIN identify, BIN diff, map-from-log occupancy heatmap, workspace export.
use serde_json::json;
use crate::logging;
use crate::ecu_database;

#[tauri::command]
pub fn identify_bin_cmd(data: Vec<u8>) -> Result<String, String> { Ok(identify_bin(&data).to_string()) }
#[tauri::command]
pub fn compare_bins_cmd(a: Vec<u8>, b: Vec<u8>) -> Result<String, String> { Ok(compare_bins(&a, &b).to_string()) }
#[tauri::command]
pub fn map_from_log_cmd() -> Result<String, String> { analyze_log().map(|v| v.to_string()) }
#[tauri::command]
pub fn export_workspace_cmd(data: Option<Vec<u8>>) -> Result<String, String> {
    let ident = data.as_deref().map(identify_bin);
    let log = analyze_log().unwrap_or_else(|e| json!({"error": e}));
    Ok(json!({"tool":"TuneItVerse","version":"3.4.0","families":ecu_database::list_supported_ecu_families(),"identify":ident,"map_from_log":log}).to_string())
}

pub fn identify_bin(data: &[u8]) -> serde_json::Value {
    let size = data.len();
    let mut size_matches = Vec::new();
    for fam in ecu_database::list_supported_ecu_families() {
        if let Some(e) = ecu_database::get_ecu_by_family(&fam) {
            if e.bin_size_bytes as usize == size { size_matches.push(e); }
        }
    }
    let family_by_size = size_matches.first().map(|e| e.ecu_family.clone());
    let display = size_matches.first().map(|e| e.display_name.clone());
    let families_same_size: Vec<String> = size_matches.iter().map(|e| e.ecu_family.clone()).collect();
    json!({"bin_size_bytes":size,"family_by_size":family_by_size,"families_same_size":families_same_size,"display_name":display,"notes": if family_by_size.is_some() { "Size matched at least one family. Confirm OS ID." } else { "Unknown size — add JSON in reference/ecu_database/." }})
}

pub fn compare_bins(a: &[u8], b: &[u8]) -> serde_json::Value {
    if a.len() != b.len() {
        return json!({"same_size":false,"len_a":a.len(),"len_b":b.len(),"diff_bytes":serde_json::Value::Null,"first_diffs":[],"message":"Images are different lengths."});
    }
    let mut diffs = 0usize; let mut first = Vec::new();
    for (i,(x,y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y { diffs += 1; if first.len() < 40 { first.push(json!({"offset":format!("0x{:06X}", i),"a":format!("{:02X}", x),"b":format!("{:02X}", y)})); } }
    }
    json!({"same_size":true,"len_a":a.len(),"len_b":b.len(),"diff_bytes":diffs,"same_bytes":a.len()-diffs,"identical":diffs==0,"first_diffs":first,"message": if diffs==0 {"Images are identical."} else {"Images differ."}})
}

fn analyze_log() -> Result<serde_json::Value, String> {
    let samples = logging::get_samples(Some(50_000));
    if samples.is_empty() { return Err("No log samples. Start a session and capture data first.".into()); }
    let mut rpm_sum=0.0; let mut n_rpm=0.0; let mut map_sum=0.0; let mut n_map=0.0;
    let mut grid = vec![vec![0u32;16];16];
    for s in &samples {
        if let Some(v)=s.values.get("rpm") { rpm_sum += v; n_rpm += 1.0; }
        if let Some(v)=s.values.get("map") { map_sum += v; n_map += 1.0; }
        let rpm = s.values.get("rpm").copied().unwrap_or(0.0);
        let mapv = s.values.get("map").copied().unwrap_or(0.0);
        let r = ((rpm/500.0).floor() as i32).clamp(0,15) as usize;
        let c = ((mapv/16.0).floor() as i32).clamp(0,15) as usize;
        grid[r][c] = grid[r][c].saturating_add(1);
    }
    let rpm_avg = if n_rpm>0.0 { rpm_sum/n_rpm } else { 0.0 };
    let map_avg = if n_map>0.0 { map_sum/n_map } else { 0.0 };
    let mut hottest = (0usize,0usize,0u32);
    for r in 0..16 { for c in 0..16 { if grid[r][c] > hottest.2 { hottest = (r,c,grid[r][c]); } } }
    Ok(json!({
        "sample_count": samples.len(),
        "rpm_avg": (rpm_avg*10.0).round()/10.0,
        "map_avg_kpa": (map_avg*10.0).round()/10.0,
        "suggested_ve_cell": {"row_rpm": ((rpm_avg/500.0).floor() as i32).clamp(0,15), "col_map": ((map_avg/16.0).floor() as i32).clamp(0,15)},
        "hottest_cell": {"row_rpm":hottest.0,"col_map":hottest.1,"hits":hottest.2},
        "occupancy_16x16": grid,
        "advice": format!("Hottest cell r{} c{} ({} hits). Mean {:.0} RPM / {:.0} kPa. Hint only — not auto-write.", hottest.0, hottest.1, hottest.2, rpm_avg, map_avg)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn identify_unknown_size() { let v = identify_bin(&[0u8;64]); assert_eq!(v["bin_size_bytes"], 64); }
    #[test] fn identify_p01_size() { let v = identify_bin(&vec![0u8;524288]); assert_eq!(v["family_by_size"], "P01_0411"); }
    #[test] fn compare_same() { let a=vec![1u8,2,3,4]; assert_eq!(compare_bins(&a,&a)["identical"], true); }
}
