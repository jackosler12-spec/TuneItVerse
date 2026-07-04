// ecu_database.rs — More robust with graceful loading and fuzzy matching

// ... (keep existing structs)

/// Safer loader that skips bad JSON entries instead of panicking
pub fn load_ecu_database() -> Vec<EcuDbEntry> {
    let mut db = Vec::new();

    let json_files = [
        ("P01", P01_JSON),
        ("EDC16", EDC16_JSON),
        ("P59", P59_JSON),
    ];

    for (name, json_str) in json_files {
        match serde_json::from_str::<EcuDbEntry>(json_str) {
            Ok(entry) => db.push(entry),
            Err(e) => eprintln!("[ECU DB] Failed to load {}: {}", name, e),
        }
    }
    db
}

/// Improved fuzzy OSID matching
pub fn get_ecu_by_os_id(os_id: &str) -> Option<EcuDbEntry> {
    let os = os_id.to_ascii_uppercase().trim_start_matches("0X").to_string();
    load_ecu_database().into_iter().find(|e| {
        e.part_numbers_or_os_ids.iter().any(|id| {
            let id_clean = id.to_ascii_uppercase().trim_start_matches("0X");
            id_clean.contains(&os) || os.contains(id_clean)
        }) || e.display_name.to_ascii_uppercase().contains(&os)
    })
}

// Expose as Tauri command
#[tauri::command]
pub fn get_ecu_info(os_id: String) -> Option<EcuDbEntry> {
    get_ecu_by_os_id(&os_id)
}