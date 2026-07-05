// Expose connection state to frontend

#[tauri::command]
async fn get_connection_state(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.connection_state.lock().map_err(|e| e.to_string())?;
    serde_json::to_string(&*guard).map_err(|e| e.to_string())
}