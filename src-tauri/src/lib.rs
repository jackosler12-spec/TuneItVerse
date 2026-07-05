// Improved ELM parsing helper

fn parse_elm_response(response: &str, pid: &str) -> Option<f64> {
    // ELM responses look like: "41 0C 1A 2B" or "SEARCHING..." or "NO DATA"
    let cleaned: String = response
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || *c == ' ')
        .collect();

    let bytes: Vec<u8> = cleaned
        .split_whitespace()
        .filter_map(|s| u8::from_str_radix(s, 16).ok())
        .collect();

    if bytes.len() < 3 || bytes[0] != 0x41 {
        return None;
    }

    match pid {
        "rpm" => {
            if bytes.len() >= 4 {
                let raw = ((bytes[2] as u16) << 8) | bytes[3] as u16;
                Some(raw as f64 / 4.0)
            } else { None }
        }
        "map" => {
            if bytes.len() >= 3 {
                Some(bytes[2] as f64 * 0.5) // kPa
            } else { None }
        }
        "afr" => {
            if bytes.len() >= 4 {
                let raw = ((bytes[2] as u16) << 8) | bytes[3] as u16;
                Some(raw as f64 / 32768.0 * 14.7) // approx AFR
            } else { None }
        }
        _ => Some(bytes.get(2).copied().unwrap_or(0) as f64),
    }
}