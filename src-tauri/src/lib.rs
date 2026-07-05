// In the ELM branch of read_live_data, replace the basic parsing with:

if let Ok(resp) = send_elm_command(state.clone(), &obd_cmd) {
    if let Some(value) = parse_elm_response(&resp, &pid) {
        result[pid.clone()] = serde_json::json!(value);
    }
}