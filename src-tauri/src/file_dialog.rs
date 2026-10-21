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

/// Largest file the recent-files list will reopen (P12b/Simos18 images are 2-4 MiB).
pub const RECENT_MAX_BYTES: u64 = 16 * 1024 * 1024;

fn allowed_exts(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "bin" => Some(&["bin", "ori", "mod"]),
        "xdf" => Some(&["xdf", "xml", "txt"]),
        "a2l" => Some(&["a2l", "txt"]),
        "csv" => Some(&["csv", "txt"]),
        _ => None,
    }
}

/// Validate a recent-file path: known kind, matching extension, regular file, size cap.
pub fn check_recent_path(path: &str, kind: &str) -> Result<std::path::PathBuf, String> {
    let k = kind.to_ascii_lowercase();
    let exts = allowed_exts(&k).ok_or_else(|| format!("Unknown recent-file kind '{}'", kind))?;
    let p = std::path::PathBuf::from(path.trim());
    let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if !exts.contains(&ext.as_str()) {
        return Err(format!("'{}' is not a {} file ({:?})", p.display(), k, exts));
    }
    let meta = std::fs::metadata(&p).map_err(|e| format!("Recent file {} is gone: {}", p.display(), e))?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", p.display()));
    }
    if meta.len() == 0 || meta.len() > RECENT_MAX_BYTES {
        return Err(format!("{} is {} bytes (allowed 1..={})", p.display(), meta.len(), RECENT_MAX_BYTES));
    }
    Ok(p)
}

/// Reopen a file from the UI's recent list (no dialog). Read-only.
#[tauri::command]
pub fn open_recent_file(path: String, kind: String) -> Result<OpenFileResult, String> {
    let p = check_recent_path(&path, &kind)?;
    let data = std::fs::read(&p).map_err(|e| format!("Failed to read {}: {}", p.display(), e))?;
    let path_s = p.display().to_string();
    if kind.eq_ignore_ascii_case("bin") {
        Ok(OpenFileResult { cancelled: false, path: Some(path_s), bytes: Some(data), text: None })
    } else {
        Ok(OpenFileResult { cancelled: false, path: Some(path_s), bytes: None, text: Some(String::from_utf8_lossy(&data).into_owned()) })
    }
}

static UNSAVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The UI reports whether the working BIN has edits that were not saved.
#[tauri::command]
pub fn set_unsaved_changes(dirty: bool) -> Result<bool, String> {
    UNSAVED.store(dirty, std::sync::atomic::Ordering::SeqCst);
    Ok(dirty)
}

pub fn has_unsaved_changes() -> bool {
    UNSAVED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Window close guard: ask before discarding unsaved BIN edits. Returns true to allow close.
pub fn confirm_close_with_unsaved() -> bool {
    if !has_unsaved_changes() {
        return true;
    }
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved BIN changes")
        .set_description("The working BIN has edits that were not saved. Close TuneItVerse and discard them?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_path_checks() {
        let dir = std::env::temp_dir().join(format!("tiv_recent_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("a.bin");
        std::fs::write(&bin, [1u8, 2, 3]).unwrap();
        let empty = dir.join("e.bin");
        std::fs::write(&empty, []).unwrap();
        let exe = dir.join("x.exe");
        std::fs::write(&exe, [1u8]).unwrap();
        assert!(check_recent_path(bin.to_str().unwrap(), "bin").is_ok());
        assert!(check_recent_path(bin.to_str().unwrap(), "BIN").is_ok());
        assert!(check_recent_path(exe.to_str().unwrap(), "bin").unwrap_err().contains("not a bin"));
        assert!(check_recent_path(bin.to_str().unwrap(), "xdf").is_err());
        assert!(check_recent_path(bin.to_str().unwrap(), "exe").unwrap_err().contains("Unknown"));
        assert!(check_recent_path(empty.to_str().unwrap(), "bin").unwrap_err().contains("allowed"));
        assert!(check_recent_path(dir.join("gone.bin").to_str().unwrap(), "bin").unwrap_err().contains("gone"));
        assert!(check_recent_path(dir.to_str().unwrap(), "bin").is_err());
        let r = open_recent_file(bin.to_str().unwrap().into(), "bin".into()).unwrap();
        assert_eq!(r.bytes.unwrap(), vec![1, 2, 3]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsaved_flag_round_trip() {
        set_unsaved_changes(true).unwrap();
        assert!(has_unsaved_changes());
        set_unsaved_changes(false).unwrap();
        assert!(!has_unsaved_changes());
        assert!(confirm_close_with_unsaved(), "no dialog when clean");
    }
    #[test]
    fn cancelled_shape() {
        let c = cancelled();
        assert!(c.cancelled);
        assert!(c.bytes.is_none());
    }
}
