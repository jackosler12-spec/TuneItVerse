// In read_live_data, add ZD30 support

// After J2534 and ELM checks, add:
if pid == "rail_pressure" || pid == "egr_duty" || pid == "injection_ms" || pid == "boost" {
    if let Ok(value) = crate::consult::read_zd30_pid(&pid) {
        result[pid.clone()] = serde_json::json!(value);
        continue;
    }
}