// xdf.rs — XDF Axis Integration improvements

use std::fs;
use std::path::Path;

// Existing parse_axes_from_xdf and structs...

/// Load XDF file for a given OSID and return table definitions with axes
pub fn load_xdf_for_os(osid: &str) -> Result<String, String> {
    // Try to find matching XDF in reference folder
    let xdf_path = format!("reference/OS-{}-*.xdf", osid);
    // For simplicity, we use a known good XDF for P01 12225074
    let default_xdf = "reference/OS-12225074-512Kb-2001-2004.xdf";

    if !Path::new(default_xdf).exists() {
        return Err("Reference XDF not found".into());
    }

    let xml = fs::read_to_string(default_xdf)
        .map_err(|e| format!("Failed to read XDF: {}", e))?;

    // In a full implementation we would parse all tables + axes from the XDF
    // For now return a structured JSON with example tables + axes
    let result = serde_json::json!({
        "osid": osid,
        "tables": [
            {
                "id": "main_ve",
                "name": "Main VE Table",
                "rows": 16,
                "cols": 16,
                "x_axis": [600, 800, 1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800, 3000, 3500, 4000, 4500],
                "y_axis": [20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170],
                "x_label": "RPM",
                "y_label": "MAP (kPa)"
            },
            {
                "id": "spark",
                "name": "Spark Advance",
                "rows": 16,
                "cols": 16,
                "x_axis": [600, 800, 1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800, 3000, 3500, 4000, 4500],
                "y_axis": [20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170],
                "x_label": "RPM",
                "y_label": "MAP (kPa)"
            }
        ]
    });

    Ok(serde_json::to_string(&result).unwrap())
}