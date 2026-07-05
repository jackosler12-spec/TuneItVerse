// xdf.rs — Full real axis parsing implementation

use quick_xml::de::from_str;
use serde::Deserialize;

// ... existing TableDef, ExtractedTable, etc. ...

#[derive(Debug, Deserialize)]
struct XdfAxis {
    #[serde(rename = "Data")]
    data: Option<String>,
    // Can be extended for <Breakpoint> style
}

#[derive(Debug, Deserialize)]
struct XdfTable {
    #[serde(rename = "XAxis")]
    x_axis: Option<XdfAxis>,
    #[serde(rename = "YAxis")]
    y_axis: Option<XdfAxis>,
}

/// Parse real axis breakpoints from XDF XML snippet
pub fn parse_axes_from_xdf(xml: &str) -> (Vec<f64>, Vec<f64>) {
    let mut axes_x = vec![];
    let mut axes_y = vec![];

    if let Ok(table) = from_str::<XdfTable>(xml) {
        if let Some(x) = table.x_axis {
            if let Some(data_str) = x.data {
                axes_x = data_str
                    .split_whitespace()
                    .filter_map(|s| s.parse::<f64>().ok())
                    .collect();
            }
        }
        if let Some(y) = table.y_axis {
            if let Some(data_str) = y.data {
                axes_y = data_str
                    .split_whitespace()
                    .filter_map(|s| s.parse::<f64>().ok())
                    .collect();
            }
        }
    }

    (axes_x, axes_y)
}

// Update extract_table to use real parsing when possible
// (for now we keep defaults but the function is ready)