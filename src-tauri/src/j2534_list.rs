//! J2534 device listing. On Windows this walks PassThruSupport registry keys.
//! Non-Windows stays honest — J2534 is a Win32 API.

pub fn parse_reg_query(stdout: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut name: Option<String> = None;
    let mut vendor: Option<String> = None;
    let mut lib: Option<String> = None;
    let flush = |name: &mut Option<String>, vendor: &mut Option<String>, lib: &mut Option<String>, out: &mut Vec<String>| {
        if name.is_some() || lib.is_some() {
            out.push(format!(
                "{} | {} | {}",
                name.take().unwrap_or_else(|| "J2534 device".into()),
                vendor.take().unwrap_or_else(|| "?".into()),
                lib.take().unwrap_or_else(|| "dll path unknown".into())
            ));
        }
    };
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() { continue; }
        if line.starts_with("HKEY_") {
            flush(&mut name, &mut vendor, &mut lib, &mut out);
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let key = parts[0].to_ascii_uppercase();
            let val = parts[2..].join(" ");
            if key == "NAME" { name = Some(val); }
            else if key == "VENDOR" { vendor = Some(val); }
            else if key == "FUNCTIONLIBRARY" || key == "FUNCTION_LIBRARY" { lib = Some(val); }
        }
    }
    flush(&mut name, &mut vendor, &mut lib, &mut out);
    out
}

#[cfg(target_os = "windows")]
fn enumerate_passthru_registry() -> Vec<String> {
    let roots = [
        r"HKLM\SOFTWARE\PassThruSupport.04.04",
        r"HKLM\SOFTWARE\WOW6432Node\PassThruSupport.04.04",
        r"HKLM\SOFTWARE\PassThruSupport.04.00",
        r"HKLM\SOFTWARE\WOW6432Node\PassThruSupport.04.00",
    ];
    let mut found = Vec::new();
    for root in roots {
        let out = std::process::Command::new("reg")
            .args(["query", root, "/s"])
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let text = String::from_utf8_lossy(&o.stdout);
                for row in parse_reg_query(&text) {
                    if !found.contains(&row) { found.push(row); }
                }
            }
        }
    }
    if found.is_empty() {
        found.push("No PassThruSupport registry keys. Install a vendor J2534 driver, then pick the .dll path.".into());
    }
    found
}

#[tauri::command]
pub fn j2534_list_devices() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        Ok(enumerate_passthru_registry())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec!["J2534 PassThru is a Windows API. Use Serial/ELM on this OS. On Windows this command walks HKLM\\SOFTWARE\\PassThruSupport.04.04.".to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::parse_reg_query;
    #[test]
    fn parses_reg_query_block() {
        let sample = "HKEY_LOCAL_MACHINE\\SOFTWARE\\PassThruSupport.04.04\\OpenPort\n    Name    REG_SZ    Tactrix OpenPort 2.0\n    Vendor    REG_SZ    Tactrix\n    FunctionLibrary    REG_SZ    C:\\Windows\\OpenPort.dll\n";
        let rows = parse_reg_query(sample);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains("Tactrix OpenPort 2.0"));
        assert!(rows[0].contains("OpenPort.dll"));
    }
}
