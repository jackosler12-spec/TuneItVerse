// consult.rs — ZD30 Diesel PID Support

pub fn get_zd30_pid(pid: &str) -> Option<u8> {
    match pid {
        "rail_pressure" => Some(0x21),   // Example Consult PID for rail pressure
        "egr_duty"      => Some(0x2C),   // EGR duty cycle
        "injection_ms"  => Some(0x1A),   // Injection duration
        "boost"         => Some(0x0B),   // Boost pressure
        "maf"           => Some(0x10),   // MAF
        _ => None,
    }
}

pub fn read_zd30_pid(pid: &str) -> Result<f64, String> {
    // In real implementation this would send Consult frame and parse response
    // For now return realistic simulated values
    match pid {
        "rail_pressure" => Ok(265.0 + rand::random::<f64>() * 35.0),
        "egr_duty"      => Ok(28.0 + rand::random::<f64>() * 30.0),
        "injection_ms"  => Ok(1.65 + rand::random::<f64>() * 0.7),
        "boost"         => Ok(0.75 + rand::random::<f64>() * 0.45),
        _ => Err("Unknown ZD30 PID".into()),
    }
}