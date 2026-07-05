.setup(|app| {
    use tauri_plugin_log::{Target, TargetKind, Builder};

    let _ = app.handle().plugin(
        Builder::new()
            .targets([
                Target::new(TargetKind::Stdout),
                Target::new(TargetKind::LogDir { file_name: None }),
            ])
            .build()
    );

    Ok(())
})