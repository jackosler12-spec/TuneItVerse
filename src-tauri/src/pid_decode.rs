fn ubyte(raw: &[u8]) -> Option<f32> { raw.get(0).map(|&b| b as f32) }

/// PID 0x2F — Fuel Level %  formula: x/2.55 (standard OBD-II)
pub fn decode_fuel_level(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x1155 — Fuel Level Sensor Volts  formula: x/51
pub fn decode_fuel_level_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x114E — Fuel Tank Pressure Sensor Volts  formula: x/51
pub fn decode_fuel_tank_press_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}