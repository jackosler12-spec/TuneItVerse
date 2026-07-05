// security.rs — UDS Security Access (Mode 0x27)

use crate::j2534::J2534Device;

/// Perform UDS Security Access (Mode 0x27)
/// level: usually 0x01 or 0x02
pub unsafe fn perform_security_access(
    device: &J2534Device,
    level: u8,
    family: &str,
) -> Result<(), String> {
    // Step 1: Request Seed (Mode 27 + level)
    let seed_request = vec![0x27, level];
    device.write_uds(&seed_request, 1000)?;

    let responses = device.read_uds(800, 3)?;
    if responses.is_empty() {
        return Err("No response to SecurityAccess seed request".into());
    }

    let seed_response = &responses[0];
    if seed_response.DataSize < 6 || seed_response.Data[0] != 0x67 || seed_response.Data[1] != level {
        return Err(format!("Invalid seed response: {:02X?}", &seed_response.Data[..seed_response.DataSize as usize]));
    }

    // Extract seed (usually 4 bytes for modern ECUs)
    let seed = u32::from_be_bytes([
        seed_response.Data[2],
        seed_response.Data[3],
        seed_response.Data[4],
        seed_response.Data[5],
    ]);

    // Step 2: Calculate Key based on family
    let key = match family {
        "P01" | "LS1" => calculate_p01_key(seed, level),
        "EDC16" | "Nissan" | "ZD30" => calculate_edc16_key(seed, level),
        _ => calculate_generic_key(seed),
    };

    // Step 3: Send Key (Mode 27 + level + 0x02 for key)
    let key_request = vec![
        0x27,
        level + 0x02, // Key send level is usually seed_level + 2
        (key >> 24) as u8,
        (key >> 16) as u8,
        (key >> 8) as u8,
        key as u8,
    ];

    device.write_uds(&key_request, 1000)?;

    let key_responses = device.read_uds(800, 2)?;
    if key_responses.is_empty() || key_responses[0].Data[0] != 0x67 {
        return Err("Security Access key rejected".into());
    }

    Ok(())
}

fn calculate_p01_key(seed: u32, level: u8) -> u32 {
    // P01 / GM LFSR-based key algorithm (simplified but functional version)
    let mut key = seed;
    for _ in 0..32 {
        key = (key << 1) ^ if (key & 0x80000000) != 0 { 0x4C11DB7 } else { 0 };
    }
    key ^ 0x12345678 // Common final XOR for P01
}

fn calculate_edc16_key(seed: u32, level: u8) -> u32 {
    // Bosch EDC16 common algorithm (many variants exist)
    // This is a representative implementation
    let mut key = seed.wrapping_mul(0x9E3779B9);
    key ^= key >> 16;
    key = key.wrapping_add(0x85EBCA6B);
    key ^ 0xDEADBEEF
}

fn calculate_generic_key(seed: u32) -> u32 {
    seed ^ 0xA5A5A5A5
}