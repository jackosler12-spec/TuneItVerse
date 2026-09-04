//! Table math used by tuners: scale, offset, smooth, STFT preview.
//! Never writes the ECU. Preview only unless the user patches the BIN.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMathRequest {
    pub values: Vec<Vec<f64>>,
    pub op: String,
    pub factor: Option<f64>,
    pub offset: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StftPreviewRequest {
    pub values: Vec<Vec<f64>>,
    pub occupancy: Vec<Vec<u32>>,
    pub stft_avg: Vec<Vec<Option<f64>>>,
    pub gain: Option<f64>,
    pub min_hits: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMathResult {
    pub values: Vec<Vec<f64>>,
    pub cells_changed: usize,
    pub message: String,
}

fn dims(values: &[Vec<f64>]) -> Result<(usize, usize), String> {
    if values.is_empty() || values[0].is_empty() {
        return Err("empty table".into());
    }
    let cols = values[0].len();
    if values.iter().any(|r| r.len() != cols) {
        return Err("ragged table".into());
    }
    Ok((values.len(), cols))
}

pub fn scale(values: &[Vec<f64>], factor: f64) -> Result<TableMathResult, String> {
    let _ = dims(values)?;
    let mut out = values.to_vec();
    let mut n = 0;
    for row in &mut out {
        for cell in row {
            let next = *cell * factor;
            if (next - *cell).abs() > 1e-12 { n += 1; }
            *cell = next;
        }
    }
    Ok(TableMathResult { values: out, cells_changed: n, message: format!("scaled * {}", factor) })
}

pub fn add(values: &[Vec<f64>], offset: f64) -> Result<TableMathResult, String> {
    let _ = dims(values)?;
    let mut out = values.to_vec();
    let mut n = 0;
    for row in &mut out {
        for cell in row {
            let next = *cell + offset;
            if (next - *cell).abs() > 1e-12 { n += 1; }
            *cell = next;
        }
    }
    Ok(TableMathResult { values: out, cells_changed: n, message: format!("offset + {}", offset) })
}

pub fn smooth(values: &[Vec<f64>]) -> Result<TableMathResult, String> {
    let (rows, cols) = dims(values)?;
    let mut out = values.to_vec();
    let mut n = 0;
    for r in 0..rows {
        for c in 0..cols {
            let mut sum = 0.0;
            let mut count = 0.0;
            for dr in -1i32..=1 {
                for dc in -1i32..=1 {
                    let rr = r as i32 + dr;
                    let cc = c as i32 + dc;
                    if rr >= 0 && cc >= 0 && (rr as usize) < rows && (cc as usize) < cols {
                        sum += values[rr as usize][cc as usize];
                        count += 1.0;
                    }
                }
            }
            let next = if count > 0.0 { sum / count } else { values[r][c] };
            if (next - values[r][c]).abs() > 1e-12 { n += 1; }
            out[r][c] = (next * 1000.0).round() / 1000.0;
        }
    }
    Ok(TableMathResult { values: out, cells_changed: n, message: "3x3 neighbour average".into() })
}

/// Preview VE-style correction from STFT occupancy. Positive STFT → add fuel.
pub fn apply_stft_preview(req: &StftPreviewRequest) -> Result<TableMathResult, String> {
    let (rows, cols) = dims(&req.values)?;
    if req.occupancy.len() != rows || req.stft_avg.len() != rows {
        return Err(format!("grid mismatch: table {}x{}, occupancy rows {}, stft rows {}", rows, cols, req.occupancy.len(), req.stft_avg.len()));
    }
    let gain = req.gain.unwrap_or(0.25).clamp(0.0, 1.0);
    let min_hits = req.min_hits.unwrap_or(3);
    let mut out = req.values.clone();
    let mut n = 0;
    for r in 0..rows {
        if req.occupancy[r].len() != cols || req.stft_avg[r].len() != cols {
            return Err("ragged occupancy/stft grid".into());
        }
        for c in 0..cols {
            if req.occupancy[r][c] < min_hits { continue; }
            if let Some(stft) = req.stft_avg[r][c] {
                let next = req.values[r][c] * (1.0 + (stft / 100.0) * gain);
                if (next - out[r][c]).abs() > 1e-9 {
                    n += 1;
                    out[r][c] = (next * 1000.0).round() / 1000.0;
                }
            }
        }
    }
    Ok(TableMathResult {
        values: out,
        cells_changed: n,
        message: format!("STFT preview gain={} min_hits={} — not written until you patch the BIN", gain, min_hits),
    })
}

pub fn apply_op(req: TableMathRequest) -> Result<TableMathResult, String> {
    match req.op.to_ascii_lowercase().as_str() {
        "scale" | "multiply" | "mul" => scale(&req.values, req.factor.unwrap_or(1.0)),
        "add" | "offset" => add(&req.values, req.offset.or(req.factor).unwrap_or(0.0)),
        "smooth" | "blur" => smooth(&req.values),
        other => Err(format!("unknown table op '{}'", other)),
    }
}

#[tauri::command]
pub fn table_math_cmd(req: TableMathRequest) -> Result<TableMathResult, String> {
    apply_op(req)
}

#[tauri::command]
pub fn apply_stft_preview_cmd(req: StftPreviewRequest) -> Result<TableMathResult, String> {
    apply_stft_preview(&req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_and_stft() {
        let v = vec![vec![100.0, 100.0], vec![100.0, 100.0]];
        let s = scale(&v, 1.1).unwrap();
        assert!((s.values[0][0] - 110.0).abs() < 1e-9);
        let req = StftPreviewRequest {
            values: v,
            occupancy: vec![vec![10, 0], vec![0, 4]],
            stft_avg: vec![vec![Some(10.0), Some(50.0)], vec![None, Some(-20.0)]],
            gain: Some(0.5),
            min_hits: Some(3),
        };
        let p = apply_stft_preview(&req).unwrap();
        assert_eq!(p.cells_changed, 2);
        assert!((p.values[0][0] - 105.0).abs() < 1e-6);
        assert!((p.values[1][1] - 90.0).abs() < 1e-6);
        assert!((p.values[0][1] - 100.0).abs() < 1e-9);
    }
}
