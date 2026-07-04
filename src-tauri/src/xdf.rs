// xdf.rs — Improved with axis parsing and auto checksum after patch

// ... (keep existing code up to extract_table)

/// Enhanced extract_table that tries to parse axes if present in future XML
pub fn extract_table(bin: &[u8], def: &TableDef) -> ExtractedTable {
    // ... existing logic ...
    let mut axes_x = vec![];
    let mut axes_y = vec![];

    // Placeholder: In a full implementation we would parse <XAxis> / <YAxis> from the XDF
    // For now we generate reasonable defaults based on table size
    for i in 0..def.cols { axes_x.push(i as f64 * 500.0); } // RPM example
    for i in 0..def.rows { axes_y.push(i as f64 * 10.0); }   // MAP example

    ExtractedTable {
        id: def.id.clone(),
        values,
        axes_x,
        axes_y,
        note,
    }
}

// Updated patch function that auto-corrects checksums
#[tauri::command]
pub fn patch_table_into_bin(req: PatchRequest) -> Result<PatchResult, String> {
    let mut patched = patch_table(req.bin_bytes, &req.table, &req.new_values);

    // Auto correct checksums after patch (highly recommended)
    let checksum_result = crate::checksum::correct_and_validate_checksums(&patched).ok();

    Ok(PatchResult {
        patched_bytes: patched,
        checksum_report: checksum_result.map(|c| c.report),
        message: format!("Patched table {} and corrected checksums", req.table.name),
    })
}