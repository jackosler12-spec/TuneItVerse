// Add this command in lib.rs

#[tauri::command]
fn load_xdf_for_os(osid: String) -> Result<String, String> {
    crate::xdf::load_xdf_for_os(&osid)
}