// ZD30 Diesel PID support in read_live_data

// Add these mappings in the J2534 / ELM section:
// "rail_pressure" => Mode 22 or Consult specific
// "egr_duty"     => Specific PID
// "injection_ms" => Specific PID

// Example extension in the mock section:
"rail_pressure" => 280.0 + rand::random::<f64>() * 40.0,  // bar
"egr_duty"     => 35.0 + rand::random::<f64>() * 25.0,   // %
"injection_ms" => 1.8 + rand::random::<f64>() * 0.6,     // ms