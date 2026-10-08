//! Driver plugin SDK.
//!
//! JSON descriptors only. A plugin can advertise transports and read/log
//! capabilities. It cannot enable a calibration write path.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPlugin {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub protocols: Vec<String>,
    pub families: Vec<String>,
    pub capabilities: Vec<String>,
    pub write_allowed: bool,
    pub notes: String,
}

const ELM327: &str = include_str!("../../reference/plugins/elm327.json");
const OBDLINK: &str = include_str!("../../reference/plugins/obdlink_mx.json");
const J2534: &str = include_str!("../../reference/plugins/j2534.json");
const FTDI: &str = include_str!("../../reference/plugins/ftdi_serial.json");

fn builtin_raw() -> [&'static str; 4] {
    [ELM327, OBDLINK, J2534, FTDI]
}

pub fn sanitize(mut plugin: DriverPlugin) -> Result<DriverPlugin, String> {
    plugin.id = plugin.id.trim().to_ascii_lowercase();
    plugin.name = plugin.name.trim().to_string();
    if plugin.id.is_empty() || plugin.id.len() > 48 {
        return Err("plugin id must be 1-48 characters".into());
    }
    if !plugin.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("plugin id must be ascii letters, digits, '-' or '_'".into());
    }
    if plugin.name.is_empty() {
        plugin.name = plugin.id.clone();
    }
    if plugin.transport.trim().is_empty() {
        return Err("plugin transport is required".into());
    }
    if plugin.capabilities.is_empty() {
        return Err("plugin needs at least one capability".into());
    }
    let banned = ["write", "flash", "unlock", "security_access"];
    for cap in &plugin.capabilities {
        let norm = cap.trim().to_ascii_lowercase();
        if banned.iter().any(|b| norm == *b) {
            return Err(format!("capability '{cap}' is not allowed on a driver plugin"));
        }
    }
    plugin.write_allowed = false;
    plugin.notes = if plugin.notes.is_empty() {
        "Driver descriptor. Cannot enable write.".into()
    } else {
        plugin.notes
    };
    Ok(plugin)
}

pub fn parse_plugin(text: &str) -> Result<DriverPlugin, String> {
    let plugin: DriverPlugin = serde_json::from_str(text).map_err(|e| format!("plugin JSON: {e}"))?;
    sanitize(plugin)
}

pub fn builtin_plugins() -> Result<Vec<DriverPlugin>, String> {
    builtin_raw().iter().map(|raw| parse_plugin(raw)).collect()
}

#[tauri::command]
pub fn list_driver_plugins() -> Result<String, String> {
    let plugins = builtin_plugins()?;
    serde_json::to_string(&serde_json::json!({
        "count": plugins.len(),
        "write_allowed": false,
        "plugins": plugins,
        "notes": "Descriptors only. A plugin cannot flip write_allowed."
    }))
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn validate_driver_plugin(text: String) -> Result<String, String> {
    let plugin = parse_plugin(&text)?;
    serde_json::to_string(&serde_json::json!({
        "ok": true,
        "write_allowed": plugin.write_allowed,
        "plugin": plugin,
        "notes": "Accepted as a read/log descriptor. Write stays off."
    }))
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_cannot_write() {
        let plugins = builtin_plugins().unwrap();
        assert!(plugins.len() >= 4);
        assert!(plugins.iter().all(|p| !p.write_allowed));
    }

    #[test]
    fn rejects_write_capability() {
        let raw = r#"{"id":"bad","name":"Bad","transport":"can","protocols":["CAN"],"families":["P01_0411"],"capabilities":["write"],"write_allowed":true,"notes":""}"#;
        assert!(parse_plugin(raw).is_err());
    }

    #[test]
    fn forces_write_flag_off() {
        let raw = r#"{"id":"bench-reader","name":"Bench","transport":"serial","protocols":["VPW"],"families":["P01_0411"],"capabilities":["identify"],"write_allowed":true,"notes":"personal"}"#;
        let plugin = parse_plugin(raw).unwrap();
        assert!(!plugin.write_allowed);
    }
}
