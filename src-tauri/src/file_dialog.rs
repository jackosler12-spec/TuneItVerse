//! Native Windows file dialogs. No ECU required.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct OpenFileResult {
    pub cancelled: bool,
    pub path: Option<String>,
    pub bytes: Option<Vec<u8>>,
    pub text: Option<String>,
}

fn cancelled() -> OpenFileResult {
    OpenFileResult { cancelled: true, path: None, bytes: None, text: None }
}

#[tauri::command]
pub fn dialog_open_file(kind: String) -> Result<OpenFileResult, String> {
    let k = kind.to_ascii_lowercase();
    let mut dlg = rfd::FileDialog::new();
    dlg = match k.as_str() {
        "bin" => dlg.add_filter("BIN image", &["bin"]).add_filter("All files", &["*"]),
        "csv" => dlg.add_filter("CSV log", &["csv", "txt"]),
        "json" => dlg.add_filter("Workspace JSON", &["json", "txt"]),
        "xdf" => dlg.add_filter("XDF / XML", &["xdf", "xml", "txt"]),
        "a2l" => dlg.add_filter("A2L", &["a2l", "txt"]),
        _ => dlg.add_filter("Text / defs", &["xdf", "xml", "a2l", "json", "csv", "txt"]).add_filter("All files", &["*"]),
    };
    let Some(path) = dlg.pick_file() else { return Ok(cancelled()); };
    let data = std::fs::read(&path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let path_s = path.display().to_string();
    if k == "bin" {
        Ok(OpenFileResult { cancelled: false, path: Some(path_s), bytes: Some(data), text: None })
    } else {
        Ok(OpenFileResult {
            cancelled: false,
            path: Some(path_s),
            bytes: None,
            text: Some(String::from_utf8_lossy(&data).into_owned()),
        })
    }
}

#[tauri::command]
pub fn dialog_save_bytes(data: Vec<u8>, default_name: Option<String>) -> Result<OpenFileResult, String> {
    let name = default_name.unwrap_or_else(|| "tuned.bin".into());
    let lower = name.to_ascii_lowercase();
    let mut dlg = rfd::FileDialog::new().set_file_name(&name);
    dlg = if lower.ends_with(".csv") {
        dlg.add_filter("CSV", &["csv"])
    } else if lower.ends_with(".json") {
        dlg.add_filter("JSON", &["json"])
    } else {
        dlg.add_filter("BIN image", &["bin"])
    };
    let Some(path) = dlg.save_file() else { return Ok(cancelled()); };
    std::fs::write(&path, &data).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(OpenFileResult {
        cancelled: false,
        path: Some(path.display().to_string()),
        bytes: None,
        text: None,
    })
}

#[tauri::command]
pub fn dialog_save_text(text: String, default_name: Option<String>) -> Result<OpenFileResult, String> {
    dialog_save_bytes(text.into_bytes(), default_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_shape() {
        let c = cancelled();
        assert!(c.cancelled);
        assert!(c.bytes.is_none());
    }
}
