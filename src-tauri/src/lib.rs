// Add this in the Builder setup

.setup(|app| {
    use tauri_plugin_log::{Target, TargetKind};

    let log_plugin = tauri_plugin_log::Builder::new()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::LogDir { file_name: None }),
        ])
        .build();

    app.handle().plugin(log_plugin)?;

    Ok(())
})