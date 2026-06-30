#![allow(unused, dead_code)]
//! pid_decode.rs — TuneItVerse / JRTuners
//!
//! Full PID decode library for GM P01/P59 ECU (J1850 VPW, Mode 0x22) and standard OBD-II Mode 0x01.
//! All formulas sourced from: reference/Parameters.Standard.xml, reference/PidDescriptions.xml,
//! reference/PidList.xml, and reference/PidParameters-CAN.XML in the TuneItVerse repo.
//!
//! ## PID namespace conventions
//! - `0x00xx` → Standard OBD-II Mode 01 PIDs (SAE J1979)
//! - `0x11xx`–`0x19xx` → GM P01/P59 proprietary Mode 22 PIDs (EE RAM parameters)
//! - `0xFCxx` → GM extended parameters
//!
//! ## Return convention
//! Every decode function returns `Option<f32>`.
//! `None` = raw bytes were too short or invalid for that PID.
//! The caller is responsible for mapping the value to its unit string (see `pid_unit()`).

use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// Raw byte helpers
// ────────────────────────────────────────────────────────────────────────────

/// Read a single unsigned byte from position 0.
#[inline]
fn ubyte(raw: &[u8]) -> Option<f32> {
    raw.get(0).map(|&b| b as f32)
}

/// Read a single signed byte from position 0.
#[inline]
fn sbyte(raw: &[u8]) -> Option<f32> {
    raw.get(0).map(|&b| b as i8 as f32)
}

/// Read a big-endian unsigned 16-bit word from positions 0–1.
#[inline]
fn u16be(raw: &[u8]) -> Option<f32> {
    if raw.len() >= 2 {
        Some(u16::from_be_bytes([raw[0], raw[1]]) as f32)
    } else {
        None
    }
}

/// Read a big-endian signed 16-bit word from positions 0–1.
#[inline]
fn s16be(raw: &[u8]) -> Option<f32> {
    if raw.len() >= 2 {
        Some(i16::from_be_bytes([raw[0], raw[1]]) as f32)
    } else {
        None
    }
}

/// Read a big-endian unsigned 32-bit word from positions 0–3.
#[inline]
fn u32be(raw: &[u8]) -> Option<f32> {
    if raw.len() >= 4 {
        Some(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as f32)
    } else {
        None
    }
}

// ────────────────────────────────────────────────────────────────────────────
// PID metadata — returned alongside decoded values for the GUI
// ────────────────────────────────────────────────────────────────────────────

/// Metadata for a PID entry: display name, unit string, and byte width.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidMeta {
    pub pid: u16,
    pub name: &'static str,
    pub unit: &'static str,
    pub bytes: u8,
}

/// Decoded PID result ready for UI consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidResult {
    pub pid: u16,
    pub name: &'static str,
    pub value: Option<f32>,
    pub unit: &'static str,
    pub raw: Vec<u8>,
}

// ────────────────────────────────────────────────────────────────────────────
// Standard OBD-II Mode 01 PIDs (0x0003 – 0x0019)
// Source: PidDescriptions.xml + Parameters.Standard.xml
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x03 — Fuel System Status
/// Byte 0 = Bank 1 status, Byte 1 = Bank 2 status. 0x01 = Open Loop, 0x02 = Closed Loop.
/// Returns raw byte 0 (caller maps to status string via `decode_fuel_system_status_str`).
pub fn decode_fuel_system_status(raw: &[u8]) -> Option<f32> {
    ubyte(raw) // pass-through: 1 = open, 2 = closed
}

pub fn decode_fuel_system_status_str(raw: &[u8]) -> &'static str {
    match raw.get(0) {
        Some(1) => "Open Loop",
        Some(2) => "Closed Loop",
        Some(4) => "Open Loop - Fault",
        Some(8) => "Open Loop - Low Temp",
        _ => "Unknown",
    }
}

/// PID 0x04 — Engine Load (Calculated)
/// Formula: `% = x / 2.55`
/// Range: 0–100 %
pub fn decode_engine_load(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x05 — ECT (Engine Coolant Temperature)
/// Formula: `°C = x - 40`
/// Range: -40 to +215 °C
pub fn decode_ect(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x06 — Short Term Fuel Trim Bank 1
/// Formula: `% = (x - 128) / 1.28`
/// 0x00 = -100 % (full lean), 0xFF = +99.22 % (full rich)
pub fn decode_stft_bank1(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x07 — Long Term Fuel Trim Bank 1
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_ltft_bank1(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x08 — Short Term Fuel Trim Bank 2
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_stft_bank2(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x09 — Long Term Fuel Trim Bank 2
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_ltft_bank2(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x0A — Fuel Pressure (gauge)
/// Formula: `kPa = x * 3`
/// Range: 0–765 kPa gauge
pub fn decode_fuel_pressure(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 3.0)
}

/// PID 0x0B — Intake Manifold Absolute Pressure (MAP)
/// Formula: `kPa = x` (direct count = kPa)
/// Range: 0–255 kPa absolute
pub fn decode_map(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x0C — Engine RPM
/// Formula: `RPM = N * 0.25`  where N = (A*256 + B) from two bytes
/// Range: 0–16,383.75 RPM
pub fn decode_engine_rpm(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 0.25)
}

/// PID 0x0D — Vehicle Speed Sensor (VSS)
/// Formula: `kph = x`  (direct count = km/h)
pub fn decode_vss(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x0E — Ignition Timing Advance (#1 Cylinder)
/// Formula: `° = (x / 2) - 64`
/// 0x00 = -64°, 0x80 = 0°, 0xFF = +63.5° BTDC
pub fn decode_timing_advance(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? / 2.0) - 64.0)
}

/// PID 0x0F — IAT (Intake Air Temperature)
/// Formula: `°C = x - 40`
pub fn decode_iat(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x10 — MAF Sensor (Mass Air Flow) — standard OBD-II
/// Formula: `g/s = x / 100`  where x is 16-bit (two bytes, A*256 + B)
pub fn decode_maf_obd(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 100.0)
}

/// PID 0x11 — Throttle Position (absolute)
/// Formula: `% = (x * 100) / 255`
pub fn decode_throttle_pos(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? * 100.0) / 255.0)
}

/// PID 0x14 — O2 Sensor Voltage Bank 1, Sensor 1 (Left Upstream) — OBD-II
/// storageType: int16 (raw pass-through volts; scaling varies by ECU)
pub fn decode_o2_b1s1_obd(raw: &[u8]) -> Option<f32> {
    s16be(raw)
}

/// PID 0x15 — O2 Sensor Voltage Bank 1, Sensor 2 (Left Downstream) — OBD-II
pub fn decode_o2_b1s2_obd(raw: &[u8]) -> Option<f32> {
    s16be(raw)
}

/// PID 0x18 — O2 Sensor Voltage Bank 2, Sensor 1 (Right Upstream) — OBD-II
pub fn decode_o2_b2s1_obd(raw: &[u8]) -> Option<f32> {
    s16be(raw)
}

/// PID 0x19 — O2 Sensor Voltage Bank 2, Sensor 2 (Right Downstream) — OBD-II
pub fn decode_o2_b2s2_obd(raw: &[u8]) -> Option<f32> {
    s16be(raw)
}

/// PID 0x1C — OBD Standards / Diagnostics type
/// Raw pass-through (see OBD-II spec for bit mapping)
pub fn decode_obd_standards(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1F — Engine Run Time (OBD-II Mode 01)
/// Formula: `seconds = A*256 + B`
pub fn decode_run_time_obd(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 Proprietary PIDs — 0x1140–0x115F (Analog sensor A/D voltages)
// Source: Parameters.Standard.xml — id 1140..115F
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x1140 — MAF Sensor (high precision, GM Mode 22)
/// Formula: `g/s = x * (512 / 65536) = x * 0.0078125`
/// 16-bit raw value from GM proprietary request
pub fn decode_maf_hi(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * (512.0 / 65536.0))
}

/// PID 0x1141 — Ignition 1 Signal Voltage
/// Formula: `V = x / 10`
/// Range: 0–25.5 V
pub fn decode_ignition_voltage(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 10.0)
}

/// PID 0x1142 — MAP Sensor Voltage (A/D)
/// Formula: `V = x / 51`
/// Range: 0–5.0 V
pub fn decode_map_volts(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1143 — TPS (Throttle Position Sensor) Voltage A/D
/// Formula: `V = x / 51`
pub fn decode_tps_volts(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1144 — A/C High Side Pressure Sensor Voltage
/// Formula: `V = x / 51`
/// Note: when used as wideband A/F: `:1 = ((x / 51.0) / 0.5) + 9.58`
pub fn decode_ac_pressure_volts(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1144 (alternate) — Wideband A/F Ratio (when wideband sensor fitted)
/// Formula: `:1 = ((x / 51.0) / 0.5) + 9.58`
pub fn decode_wideband_afr(raw: &[u8]) -> Option<f32> {
    Some(((ubyte(raw)? / 51.0) / 0.5) + 9.58)
}

/// PID 0x1145 — O2 Sensor Voltage Bank 1, Sensor 1 (Left Front, GM)
/// Formula: `mV = x / 0.2304`
pub fn decode_o2_b1s1(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x1146 — O2 Sensor Voltage Bank 1, Sensor 2 (Left Rear, GM)
/// Formula: `mV = x / 0.2304`
pub fn decode_o2_b1s2(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x1148 — O2 Sensor Voltage Bank 2, Sensor 1 (Right Front, GM)
/// Formula: `mV = x / 0.2304`
pub fn decode_o2_b2s1(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x1149 — O2 Sensor Voltage Bank 2, Sensor 2 (Right Rear, GM)
/// Formula: `mV = x / 0.2304`
pub fn decode_o2_b2s2(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x114B — EGR Position Sensor Voltage
/// Formula: `V = x / 51`
/// Note: when used as wideband A/F: `:1 = ((x / 51.0) / 0.5) + 9.58`
pub fn decode_egr_sensor_volts(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x114D — GEN F-Terminal Signal (Alternator Duty Cycle)
/// Formula: `% = x / 2.55`
pub fn decode_gen_f_terminal(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x114E — Fuel Tank Pressure Sensor Voltage
/// Formula: `V = x / 51`
pub fn decode_fuel_tank_press_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1151 — Accelerator Pedal Position (APP)
/// Formula: `% = x * (100 / 256)`
pub fn decode_app(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * (100.0 / 256.0))
}

/// PID 0x1155 — Fuel Level Sensor Voltage (Left Tank)
/// Formula: `V = x / 51`
pub fn decode_fuel_level_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x115C — Engine Oil Pressure Sensor Voltage
/// Formula: `V = x / 51`
/// Alternate kPa formula: `kPa = (x * 4.34) - 104.2`
pub fn decode_oil_pressure_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x115C (alternate) — Engine Oil Pressure in kPa
/// Formula: `kPa = (x * 4.34) - 104.2`
pub fn decode_oil_pressure_kpa(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? * 4.34) - 104.2)
}

/// PID 0x115E — CMP (Cam Position) Sensor Raw Count
/// Formula: raw pass-through
pub fn decode_cam_sensor(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 Proprietary PIDs — 0x116F–0x11BD (Engine operation params)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x116F — Startup ECT (Engine Coolant Temperature at Start)
/// Formula: `°C = x - 40`
pub fn decode_startup_ect(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x1170 — EVAP Purge Solenoid Command (Duty Cycle)
/// Formula: `% = x / 2.55`
pub fn decode_evap_purge_dc(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x1171 — EGR Duty Cycle Command
/// Formula: `% = x / 2.55`
pub fn decode_egr_dc(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x1176 — Learned IAC (Idle Air Control) Position
/// Formula: `steps = x` (direct count, 0–255 steps)
pub fn decode_iac_learned(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1179 — Current IAC (Idle Air Control) Position
/// Formula: `steps = x`
pub fn decode_iac_current(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1190 — Fuel Trim Cell
/// Raw pass-through (cell number 0–15 for VE table lookup)
pub fn decode_fuel_trim_cell(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1192 — Desired Idle Speed
/// Formula: `RPM = x * 12.5`
/// Range: 0–3187.5 RPM
pub fn decode_desired_idle_rpm(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 12.5)
}

/// PID 0x119D — Barometric Pressure (sampled at startup)
/// Formula: `kPa = (x + 28) / 2.71`
pub fn decode_baro(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? + 28.0) / 2.71)
}

/// PID 0x119E — Target A/F Ratio
/// Formula: `:1 = x / 10`
pub fn decode_target_afr(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 10.0)
}

/// PID 0x119F — Engine Oil Life Remaining
/// Formula: `% = (x / 2.55) + 0.5`
pub fn decode_oil_life(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? / 2.55) + 0.5)
}

/// PID 0x11A1 — Engine Run Time (GM Mode 22)
/// Formula: `minutes = x / 60`  (16-bit, raw = seconds)
pub fn decode_engine_run_time(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 60.0)
}

/// PID 0x11A3 — Calculated Catalytic Converter Temperature
/// Formula: `°C = x * (2050 / 256) = x * 8.0078`
pub fn decode_calc_cat_temp(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * (2050.0 / 256.0))
}

/// PID 0x11A6 — Knock Retard
/// Formula: `° = x * 0.0879`
pub fn decode_knock_retard(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 0.0879)
}

/// PID 0x11BD — EGR Test Count
/// Raw pass-through (diagnostic use)
pub fn decode_egr_test_count(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Misfire detection PIDs (0x11EA–0x11FB, 0x1201–0x1208)
// ────────────────────────────────────────────────────────────────────────────

/// PIDs 0x11EA–0x11ED — Misfire Current Cylinders 5–8
/// Raw pass-through (count per 1000 revolutions)
pub fn decode_misfire_current(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PIDs 0x11F8–0x11FB — Misfire History Cylinders 5–8
/// 16-bit raw cumulative count
pub fn decode_misfire_history(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

/// PIDs 0x1201–0x1204 — Misfire Current Cylinders 1–4
pub fn decode_misfire_current_cyl1_4(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PIDs 0x1205–0x1208 — Misfire History Cylinders 1–4
pub fn decode_misfire_history_cyl1_4(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Fuel Trim averaged / cell PIDs (0x120A–0x123A)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x120A — Short Term FT Average (all cells)
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_stft_avg(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x120B — Long Term FT Average (all cells)
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_ltft_avg(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x120C — Short Term FT Average (cell-based, alternate)
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_stft_avg_cell(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x123A — Long Term FT Average (cell-based)
/// Formula: `% = (x - 128) / 1.28`
pub fn decode_ltft_avg_cell(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x1227 — Total Misfire Fail Count
pub fn decode_misfire_fail_total(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

/// PID 0x1228 — Total Misfire Pass Count
pub fn decode_misfire_pass_total(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

/// PID 0x122A — Cycles of Misfire Data
pub fn decode_misfire_cycles(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1232 — Warm-Ups Without Emission Faults
pub fn decode_warmups_no_emission_fault(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1233 — Warm-Ups Without Non-Emission Faults
pub fn decode_warmups_no_fault(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1234 — Mileage Since DTC Cleared
/// Formula: `km = x * 5`  (16-bit)
pub fn decode_mileage_since_clear(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 5.0)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — MAF frequency & injector pulsewidth (0x1250–0x125F)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x1250 — MAF Sensor Frequency
/// Formula: `Hz = x / 2.048`  (16-bit raw)
pub fn decode_maf_freq(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 2.048)
}

/// PID 0x125A — Injector Pulsewidth Bank 1 (Left Bank Average)
/// Formula: `ms = x / 65.535`  (16-bit raw)
pub fn decode_inj_pw_bank1(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 65.535)
}

/// PID 0x125B — Injector Pulsewidth Bank 2 (Right Bank Average)
/// Formula: `ms = x / 65.535`  (16-bit raw)
pub fn decode_inj_pw_bank2(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 65.535)
}

/// PID 0x125D — Knock Retard (raw, signed offset from 128)
/// Formula: `raw = x - 128`
pub fn decode_knock_retard_raw(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 128.0)
}

/// PID 0x125E — Knock Count (cumulative)
/// Raw pass-through
pub fn decode_knock_count(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x125F — Delta Bearing (knock reference offset)
/// Raw pass-through
pub fn decode_delta_bearing(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — TAC / APP / TPS system (0x12B0–0x12BD)
// Source: Parameters.Standard.xml — id 12B0..12BD
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x12B0 — Accelerator Pedal Position (normalized)
/// Formula: `% = x / 1.11`
pub fn decode_app_normalized(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 1.11)
}

/// PID 0x12B1 — TP Indicated Angle (16-bit)
/// Formula: `% = x * 5 / 256`
pub fn decode_tp_indicated(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 5.0 / 256.0)
}

/// PID 0x12B4 — APP Sensor 1 (percent, raw normalized)
/// Formula: `% = x` (direct count)
pub fn decode_app1_pct(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x12B5 — APP Sensor 2
pub fn decode_app2_pct(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x12B6 — APP Sensor 3
pub fn decode_app3_pct(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x12B7 — TP Sensor 1 (percent)
pub fn decode_tp1_pct(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x12B8 — TP Sensor 2 (percent)
pub fn decode_tp2_pct(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x12B9 — TP Sensor 2 (voltage)
/// Formula: `V = x / 51`
pub fn decode_tp2_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x12BA — TP Sensor 1 (voltage)
/// Formula: `V = x / 51`
pub fn decode_tp1_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x12BB — APP Sensor 3 (voltage)
/// Formula: `V = x / 51`
pub fn decode_app3_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x12BC — APP Sensor 2 (voltage)
/// Formula: `V = x / 51`
pub fn decode_app2_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x12BD — APP Sensor 1 (voltage)
/// Formula: `V = x / 51`
pub fn decode_app1_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Fuel level / EVAP / ethanol (0x12C5–0x12F3)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x12C5 — Fuel Level (percent)
/// Formula: `% = x / 2.55`
pub fn decode_fuel_level(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x12E3 — Fuel Tank Pressure Sensor (mmHg, EVAP diagnostic)
/// Formula: `mmHg = -((x / 5.46) - 13.995)`
pub fn decode_fuel_tank_press_mmhg(raw: &[u8]) -> Option<f32> {
    Some(-((ubyte(raw)? / 5.46) - 13.995))
}

/// PID 0x12F3 — Ethanol Content
/// Formula: `% = x / 2.55`
pub fn decode_ethanol_content(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x12F7 — Desired IAC Position (from commanded idle)
/// Formula: `counts = x`
pub fn decode_desired_iac(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Cruise control disengage history (0x1315–0x131C)
// ────────────────────────────────────────────────────────────────────────────

/// PIDs 0x1315–0x131C — Cruise Disengage History 1–8
/// Raw pass-through (boolean flags)
pub fn decode_cruise_disengage(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Fuel tank capacity / TAC angles (0x131D–0x132C)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x131D — Fuel Tank Rated Capacity
/// Formula: `L = x / 64`  (16-bit)
pub fn decode_fuel_tank_capacity(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 64.0)
}

/// PID 0x131E — TP Desired Angle (TAC system)
/// Formula: `% = x * 5 / 256`  (16-bit)
pub fn decode_tp_desired(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 5.0 / 256.0)
}

/// PID 0x131F — APP Indicated Angle (TAC system)
/// Formula: `% = x * 5 / 256`  (16-bit)
pub fn decode_app_indicated(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 5.0 / 256.0)
}

/// PID 0x132A — Fuel Tank Level Remaining
/// Formula: `L = x / 64`  (16-bit)
pub fn decode_fuel_remaining(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 64.0)
}

/// PID 0x132B — CMP Sensor High-to-Low transition count
pub fn decode_cmp_high_to_low(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

/// PID 0x132C — CMP Sensor Low-to-High transition count
pub fn decode_cmp_low_to_high(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Torque / HO2S heater (0x1336–0x1485)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x1336 — Torque Request Signal
/// Formula: `N·m = x * 0.02646484375`  (16-bit)
pub fn decode_torque_request(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 0.02646484375)
}

/// PID 0x1337 — Torque Delivered Signal
/// Formula: `N·m = x * 0.02646484375`  (16-bit)
pub fn decode_torque_delivered(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? * 0.02646484375)
}

/// PID 0x1338 — Fuel Level Sensor (Right/Secondary Tank)
/// Formula: `V = x / 51`
pub fn decode_fuel_level_right_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x139C — Ethanol Content Sensor Frequency
/// Formula: `Hz = x`  (direct count)
pub fn decode_ethanol_freq(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1482 — HO2S Heater Current Bank 1 Sensor 1
/// Formula: `A = x / 50`  (16-bit)
pub fn decode_ho2s_heater_b1s1(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 50.0)
}

/// PID 0x1484 — HO2S Heater Current Bank 2 Sensor 1
/// Formula: `A = x / 50`  (16-bit)
pub fn decode_ho2s_heater_b2s1(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 50.0)
}

/// PID 0x1485 — HO2S Heater Current Bank 1 Sensor 2
/// Formula: `A = x / 50`  (16-bit)
pub fn decode_ho2s_heater_b1s2(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 50.0)
}

/// PID 0x1617 — Desired IAC Airflow
/// Formula: `g/s = x / 1024`  (16-bit)
pub fn decode_desired_iac_airflow(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 1024.0)
}

// ────────────────────────────────────────────────────────────────────────────
// GM P01/P59 — Transmission PIDs (0x1922–0x19F5)
// ────────────────────────────────────────────────────────────────────────────

/// PID 0x1926 — TCC (Torque Converter Clutch) Mode
/// Raw pass-through (0=off, 1=apply, 2=release, etc.)
pub fn decode_tcc_mode(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x192C — Trans Range
/// Raw pass-through (P/R/N/D/3/2/1 bitmask)
pub fn decode_trans_range(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1940 — Transmission Fluid Temperature
/// Formula: `°C = x - 40`
pub fn decode_trans_fluid_temp(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x1941 — Input Shaft Speed (raw counts)
pub fn decode_input_shaft_speed(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

/// PID 0x199A — Current Gear
/// Raw pass-through (1–4 or 1–6)
pub fn decode_current_gear(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x19F4 — Transmission Gear Ratio (16-bit raw)
pub fn decode_trans_ratio(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

/// PID 0x19F5 — Transmission Gear (16-bit raw, used in some P59 variants)
pub fn decode_trans_gear(raw: &[u8]) -> Option<f32> {
    u16be(raw)
}

// ────────────────────────────────────────────────────────────────────────────
// GM extended PIDs — 0xFCxx
// ────────────────────────────────────────────────────────────────────────────

/// PID 0xFC05 — EGR PWM Duty Cycle
/// Formula: `% = x * (100 / 256)`
pub fn decode_egr_pwm_dc(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * (100.0 / 256.0))
}

/// PID 0xFC05 (16-bit variant) — Target Equivalence Ratio
/// Formula: `EQR = x / 1024`  (16-bit)
pub fn decode_target_eqr(raw: &[u8]) -> Option<f32> {
    Some(u16be(raw)? / 1024.0)
}

// ────────────────────────────────────────────────────────────────────────────
// Dispatch table — decode any PID from its ID and raw bytes
// ────────────────────────────────────────────────────────────────────────────

/// Decode a PID given its 16-bit ID and raw response bytes.
///
/// Returns a `PidResult` with the scaled value and metadata.
/// PIDs not in the table return `value: None` with a placeholder name.
pub fn decode_pid(pid: u16, raw: &[u8]) -> PidResult {
    let (name, unit, value): (&'static str, &'static str, Option<f32>) = match pid {
        // ── OBD-II Mode 01 ──────────────────────────────────────────────────
        0x0003 => ("Fuel System Status",               "raw",    decode_fuel_system_status(raw)),
        0x0004 => ("Engine Load",                      "%",      decode_engine_load(raw)),
        0x0005 => ("ECT",                              "°C",     decode_ect(raw)),
        0x0006 => ("STFT Bank 1",                      "%",      decode_stft_bank1(raw)),
        0x0007 => ("LTFT Bank 1",                      "%",      decode_ltft_bank1(raw)),
        0x0008 => ("STFT Bank 2",                      "%",      decode_stft_bank2(raw)),
        0x0009 => ("LTFT Bank 2",                      "%",      decode_ltft_bank2(raw)),
        0x000A => ("Fuel Pressure",                    "kPa",    decode_fuel_pressure(raw)),
        0x000B => ("MAP",                              "kPa",    decode_map(raw)),
        0x000C => ("Engine RPM",                       "RPM",    decode_engine_rpm(raw)),
        0x000D => ("Vehicle Speed",                    "kph",    decode_vss(raw)),
        0x000E => ("Ignition Timing Advance",          "°",      decode_timing_advance(raw)),
        0x000F => ("IAT",                              "°C",     decode_iat(raw)),
        0x0010 => ("MAF",                              "g/s",    decode_maf_obd(raw)),
        0x0011 => ("Throttle Position",                "%",      decode_throttle_pos(raw)),
        0x0014 => ("O2 B1S1 (OBD)",                   "V",      decode_o2_b1s1_obd(raw)),
        0x0015 => ("O2 B1S2 (OBD)",                   "V",      decode_o2_b1s2_obd(raw)),
        0x0018 => ("O2 B2S1 (OBD)",                   "V",      decode_o2_b2s1_obd(raw)),
        0x0019 => ("O2 B2S2 (OBD)",                   "V",      decode_o2_b2s2_obd(raw)),
        0x001C => ("OBD Standards",                    "raw",    decode_obd_standards(raw)),
        0x001F => ("Engine Run Time (OBD)",            "s",      decode_run_time_obd(raw)),

        // ── GM P01/P59 Mode 22 — analog sensors ─────────────────────────────
        0x1140 => ("MAF (Hi-Precision)",               "g/s",    decode_maf_hi(raw)),
        0x1141 => ("Ignition Voltage",                 "V",      decode_ignition_voltage(raw)),
        0x1142 => ("MAP Sensor Voltage",               "V",      decode_map_volts(raw)),
        0x1143 => ("TPS Voltage",                      "V",      decode_tps_volts(raw)),
        0x1144 => ("A/C Pressure Sensor Voltage",      "V",      decode_ac_pressure_volts(raw)),
        0x1145 => ("O2 B1S1 (GM)",                     "mV",     decode_o2_b1s1(raw)),
        0x1146 => ("O2 B1S2 (GM)",                     "mV",     decode_o2_b1s2(raw)),
        0x1148 => ("O2 B2S1 (GM)",                     "mV",     decode_o2_b2s1(raw)),
        0x1149 => ("O2 B2S2 (GM)",                     "mV",     decode_o2_b2s2(raw)),
        0x114B => ("EGR Sensor Voltage",               "V",      decode_egr_sensor_volts(raw)),
        0x114D => ("GEN F-Terminal Signal",            "%",      decode_gen_f_terminal(raw)),
        0x114E => ("Fuel Tank Pressure Sensor",        "V",      decode_fuel_tank_press_v(raw)),
        0x1151 => ("APP Position",                     "%",      decode_app(raw)),
        0x1155 => ("Fuel Level Sensor (Left)",         "V",      decode_fuel_level_v(raw)),
        0x115C => ("Engine Oil Pressure",              "V",      decode_oil_pressure_v(raw)),
        0x115E => ("CMP Sensor Raw",                   "raw",    decode_cam_sensor(raw)),

        // ── GM P01/P59 — engine operation ────────────────────────────────────
        0x116F => ("Startup ECT",                      "°C",     decode_startup_ect(raw)),
        0x1170 => ("EVAP Purge Solenoid DC",           "%",      decode_evap_purge_dc(raw)),
        0x1171 => ("EGR Duty Cycle",                   "%",      decode_egr_dc(raw)),
        0x1176 => ("IAC Learned Position",             "steps",  decode_iac_learned(raw)),
        0x1179 => ("IAC Current Position",             "steps",  decode_iac_current(raw)),
        0x1190 => ("Fuel Trim Cell",                   "cell",   decode_fuel_trim_cell(raw)),
        0x1192 => ("Desired Idle Speed",               "RPM",    decode_desired_idle_rpm(raw)),
        0x119D => ("Barometric Pressure",              "kPa",    decode_baro(raw)),
        0x119E => ("Target A/F Ratio",                 ":1",     decode_target_afr(raw)),
        0x119F => ("Engine Oil Life",                  "%",      decode_oil_life(raw)),
        0x11A1 => ("Engine Run Time",                  "min",    decode_engine_run_time(raw)),
        0x11A3 => ("Calculated Cat Temp",              "°C",     decode_calc_cat_temp(raw)),
        0x11A6 => ("Knock Retard",                     "°",      decode_knock_retard(raw)),
        0x11BD => ("EGR Test Count",                   "raw",    decode_egr_test_count(raw)),

        // ── GM P01/P59 — misfire ─────────────────────────────────────────────
        0x11EA => ("Misfire Current Cyl 5",            "cnt",    decode_misfire_current(raw)),
        0x11EB => ("Misfire Current Cyl 6",            "cnt",    decode_misfire_current(raw)),
        0x11EC => ("Misfire Current Cyl 7",            "cnt",    decode_misfire_current(raw)),
        0x11ED => ("Misfire Current Cyl 8",            "cnt",    decode_misfire_current(raw)),
        0x11F8 => ("Misfire History Cyl 5",            "cnt",    decode_misfire_history(raw)),
        0x11F9 => ("Misfire History Cyl 6",            "cnt",    decode_misfire_history(raw)),
        0x11FA => ("Misfire History Cyl 7",            "cnt",    decode_misfire_history(raw)),
        0x11FB => ("Misfire History Cyl 8",            "cnt",    decode_misfire_history(raw)),
        0x1201 => ("Misfire Current Cyl 1",            "cnt",    decode_misfire_current_cyl1_4(raw)),
        0x1202 => ("Misfire Current Cyl 2",            "cnt",    decode_misfire_current_cyl1_4(raw)),
        0x1203 => ("Misfire Current Cyl 3",            "cnt",    decode_misfire_current_cyl1_4(raw)),
        0x1204 => ("Misfire Current Cyl 4",            "cnt",    decode_misfire_current_cyl1_4(raw)),
        0x1205 => ("Misfire History Cyl 1",            "cnt",    decode_misfire_history_cyl1_4(raw)),
        0x1206 => ("Misfire History Cyl 2",            "cnt",    decode_misfire_history_cyl1_4(raw)),
        0x1207 => ("Misfire History Cyl 3",            "cnt",    decode_misfire_history_cyl1_4(raw)),
        0x1208 => ("Misfire History Cyl 4",            "cnt",    decode_misfire_history_cyl1_4(raw)),
        0x1227 => ("Total Misfire Fail",               "cnt",    decode_misfire_fail_total(raw)),
        0x1228 => ("Total Misfire Pass",               "cnt",    decode_misfire_pass_total(raw)),
        0x122A => ("Cycles of Misfire Data",           "cnt",    decode_misfire_cycles(raw)),

        // ── GM P01/P59 — fuel trim / diagnostics ────────────────────────────
        0x120A => ("STFT Average",                     "%",      decode_stft_avg(raw)),
        0x120B => ("LTFT Average",                     "%",      decode_ltft_avg(raw)),
        0x120C => ("STFT Average (cell)",              "%",      decode_stft_avg_cell(raw)),
        0x123A => ("LTFT Average (cell)",              "%",      decode_ltft_avg_cell(raw)),
        0x1232 => ("Warm-ups w/o Emission Faults",     "cnt",    decode_warmups_no_emission_fault(raw)),
        0x1233 => ("Warm-ups w/o Non-Emission Faults", "cnt",    decode_warmups_no_fault(raw)),
        0x1234 => ("Mileage Since DTC Clear",          "km",     decode_mileage_since_clear(raw)),

        // ── GM P01/P59 — MAF frequency / injector ───────────────────────────
        0x1250 => ("MAF Sensor Frequency",             "Hz",     decode_maf_freq(raw)),
        0x125A => ("Injector PW Bank 1",               "ms",     decode_inj_pw_bank1(raw)),
        0x125B => ("Injector PW Bank 2",               "ms",     decode_inj_pw_bank2(raw)),
        0x125D => ("Knock Retard Raw",                 "raw",    decode_knock_retard_raw(raw)),
        0x125E => ("Knock Count",                      "cnt",    decode_knock_count(raw)),
        0x125F => ("Delta Bearing",                    "raw",    decode_delta_bearing(raw)),

        // ── GM P01/P59 — TAC / APP / TPS ────────────────────────────────────
        0x12B0 => ("APP Normalized",                   "%",      decode_app_normalized(raw)),
        0x12B1 => ("TP Indicated Angle",               "%",      decode_tp_indicated(raw)),
        0x12B4 => ("APP Sensor 1 (%)",                 "%",      decode_app1_pct(raw)),
        0x12B5 => ("APP Sensor 2 (%)",                 "%",      decode_app2_pct(raw)),
        0x12B6 => ("APP Sensor 3 (%)",                 "%",      decode_app3_pct(raw)),
        0x12B7 => ("TP Sensor 1 (%)",                  "%",      decode_tp1_pct(raw)),
        0x12B8 => ("TP Sensor 2 (%)",                  "%",      decode_tp2_pct(raw)),
        0x12B9 => ("TP Sensor 2 (V)",                  "V",      decode_tp2_v(raw)),
        0x12BA => ("TP Sensor 1 (V)",                  "V",      decode_tp1_v(raw)),
        0x12BB => ("APP Sensor 3 (V)",                 "V",      decode_app3_v(raw)),
        0x12BC => ("APP Sensor 2 (V)",                 "V",      decode_app2_v(raw)),
        0x12BD => ("APP Sensor 1 (V)",                 "V",      decode_app1_v(raw)),

        // ── GM P01/P59 — fuel / EVAP / ethanol ──────────────────────────────
        0x12C5 => ("Fuel Level",                       "%",      decode_fuel_level(raw)),
        0x12E3 => ("Fuel Tank Pressure (EVAP)",        "mmHg",   decode_fuel_tank_press_mmhg(raw)),
        0x12F3 => ("Ethanol Content",                  "%",      decode_ethanol_content(raw)),
        0x12F7 => ("Desired IAC Position",             "counts", decode_desired_iac(raw)),

        // ── GM P01/P59 — fuel tank / angles ─────────────────────────────────
        0x131D => ("Fuel Tank Capacity",               "L",      decode_fuel_tank_capacity(raw)),
        0x131E => ("TP Desired Angle",                 "%",      decode_tp_desired(raw)),
        0x131F => ("APP Indicated Angle",              "%",      decode_app_indicated(raw)),
        0x132A => ("Fuel Remaining",                   "L",      decode_fuel_remaining(raw)),
        0x132B => ("CMP High-to-Low",                  "cnt",    decode_cmp_high_to_low(raw)),
        0x132C => ("CMP Low-to-High",                  "cnt",    decode_cmp_low_to_high(raw)),

        // ── GM P01/P59 — torque / HO2S heater ───────────────────────────────
        0x1336 => ("Torque Request",                   "N·m",    decode_torque_request(raw)),
        0x1337 => ("Torque Delivered",                 "N·m",    decode_torque_delivered(raw)),
        0x1338 => ("Fuel Level Sensor Right",          "V",      decode_fuel_level_right_v(raw)),
        0x139C => ("Ethanol Sensor Frequency",         "Hz",     decode_ethanol_freq(raw)),
        0x1482 => ("HO2S Heater B1S1",                 "A",      decode_ho2s_heater_b1s1(raw)),
        0x1484 => ("HO2S Heater B2S1",                 "A",      decode_ho2s_heater_b2s1(raw)),
        0x1485 => ("HO2S Heater B1S2",                 "A",      decode_ho2s_heater_b1s2(raw)),
        0x1617 => ("Desired IAC Airflow",              "g/s",    decode_desired_iac_airflow(raw)),

        // ── GM P01/P59 — transmission ────────────────────────────────────────
        0x1926 => ("TCC Mode",                         "raw",    decode_tcc_mode(raw)),
        0x192C => ("Trans Range",                      "raw",    decode_trans_range(raw)),
        0x1940 => ("Trans Fluid Temp",                 "°C",     decode_trans_fluid_temp(raw)),
        0x1941 => ("Input Shaft Speed",                "raw",    decode_input_shaft_speed(raw)),
        0x199A => ("Current Gear",                     "gear",   decode_current_gear(raw)),
        0x19F4 => ("Trans Ratio",                      "raw",    decode_trans_ratio(raw)),
        0x19F5 => ("Trans Gear (P59)",                 "gear",   decode_trans_gear(raw)),

        // ── GM extended ──────────────────────────────────────────────────────
        0xFC05 => ("EGR PWM Duty Cycle",               "%",      decode_egr_pwm_dc(raw)),

        _ => ("Unknown PID", "raw", raw.get(0).map(|&b| b as f32)),
    };

    PidResult {
        pid,
        name,
        value,
        unit,
        raw: raw.to_vec(),
    }
}

/// Convenience: decode a batch of `(pid, raw)` pairs in one call.
/// Useful for processing a full Mode-22 response frame dump.
pub fn decode_pid_batch(requests: &[(u16, Vec<u8>)]) -> Vec<PidResult> {
    requests.iter().map(|(pid, raw)| decode_pid(*pid, raw)).collect()
}

// ────────────────────────────────────────────────────────────────────────────
// Unit helper
// ────────────────────────────────────────────────────────────────────────────

/// Return the display unit string for a known PID.
pub fn pid_unit(pid: u16) -> &'static str {
    decode_pid(pid, &[]).unit
}

/// Return the display name string for a known PID.
pub fn pid_name(pid: u16) -> &'static str {
    decode_pid(pid, &[]).name
}

// ────────────────────────────────────────────────────────────────────────────
// Bit-mapped boolean PIDs — separate decode for status/flag registers
// Source: Parameters.Standard.xml bitmapped=True entries
// ────────────────────────────────────────────────────────────────────────────

/// Decoded flag register for PID 0x1100 (A/C & PCM status byte)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pid1100Flags {
    pub ac_request: bool,           // bit 0
    pub ac_clutch_feedback: bool,   // bit 1
    pub pcm_reset: bool,            // bit 2
    pub ac_relay_command: bool,     // bit 3
    pub gen_l_terminal: bool,       // bit 7
}

pub fn decode_pid1100(raw: &[u8]) -> Option<Pid1100Flags> {
    let b = *raw.get(0)?;
    Some(Pid1100Flags {
        ac_request:         b & (1 << 0) != 0,
        ac_clutch_feedback: b & (1 << 1) != 0,
        pcm_reset:          b & (1 << 2) != 0,
        ac_relay_command:   b & (1 << 3) != 0,
        gen_l_terminal:     b & (1 << 7) != 0,
    })
}

/// Decoded flag register for PID 0x1102 (TCC/traction/brake status byte)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pid1102Flags {
    pub traction_control_active: bool,     // bit 0
    pub vtd_fuel_disable: bool,            // bit 1
    pub tcc_enable_solenoid: bool,         // bit 2
    pub tcc_cruise_brake: bool,            // bit 3
    pub powertrain_chassis_pitch: bool,    // bit 5
    pub extended_travel_brake: bool,       // bit 6
}

pub fn decode_pid1102(raw: &[u8]) -> Option<Pid1102Flags> {
    let b = *raw.get(0)?;
    Some(Pid1102Flags {
        traction_control_active:  b & (1 << 0) != 0,
        vtd_fuel_disable:         b & (1 << 1) != 0,
        tcc_enable_solenoid:      b & (1 << 2) != 0,
        tcc_cruise_brake:         b & (1 << 3) != 0,
        powertrain_chassis_pitch: b & (1 << 5) != 0,
        extended_travel_brake:    b & (1 << 6) != 0,
    })
}

/// Decoded flag register for PID 0x1103 (MIL / oil / reduced power)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pid1103Flags {
    pub mil_command: bool,          // bit 1
    pub oil_level_low: bool,        // bit 5
    pub reduced_engine_power: bool, // bit 7
}

pub fn decode_pid1103(raw: &[u8]) -> Option<Pid1103Flags> {
    let b = *raw.get(0)?;
    Some(Pid1103Flags {
        mil_command:          b & (1 << 1) != 0,
        oil_level_low:        b & (1 << 5) != 0,
        reduced_engine_power: b & (1 << 7) != 0,
    })
}

/// Decoded flag register for PID 0x1105 (loop status / decel / fuel trim learn)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pid1105Flags {
    pub closed_loop: bool,      // bit 0 — 0=open, 1=closed
    pub fuel_trim_learn: bool,  // bit 5
    pub cold_startup: bool,     // bit 6
    pub decel: bool,            // bit 7
}

pub fn decode_pid1105(raw: &[u8]) -> Option<Pid1105Flags> {
    let b = *raw.get(0)?;
    Some(Pid1105Flags {
        closed_loop:      b & (1 << 0) != 0,
        fuel_trim_learn:  b & (1 << 5) != 0,
        cold_startup:     b & (1 << 6) != 0,
        decel:            b & (1 << 7) != 0,
    })
}

/// Decoded flag register for PID 0x1116 (cruise/TAC switch inputs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pid1116Flags {
    pub cruise_on_off: bool,        // bit 0 (inferred)
    pub stoplamp_pedal: bool,       // bit 1 (inferred)
    pub cruise_set_coast: bool,     // bit 4 (inferred)
    pub cruise_resume_accel: bool,  // bit 6
    pub tp_sensors_agree: bool,     // bit 5 (inferred)
    pub app_sensors_agree: bool,    // bit 3 (inferred)
}

pub fn decode_pid1116(raw: &[u8]) -> Option<Pid1116Flags> {
    let b = *raw.get(0)?;
    Some(Pid1116Flags {
        cruise_on_off:        b & (1 << 0) != 0,
        stoplamp_pedal:       b & (1 << 1) != 0,
        app_sensors_agree:    b & (1 << 3) != 0,
        cruise_set_coast:     b & (1 << 4) != 0,
        tp_sensors_agree:     b & (1 << 5) != 0,
        cruise_resume_accel:  b & (1 << 6) != 0,
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_rpm() {
        // 0x1B58 = 7000 raw → 7000 * 0.25 = 1750 RPM
        let raw = [0x1B, 0x58];
        let v = decode_engine_rpm(&raw).unwrap();
        assert!((v - 1750.0).abs() < 0.1, "RPM = {v}");
    }

    #[test]
    fn test_ect() {
        // 0x69 = 105 raw → 105 - 40 = 65°C
        assert!((decode_ect(&[0x69]).unwrap() - 65.0).abs() < 0.01);
    }

    #[test]
    fn test_timing_advance() {
        // 0x80 = 128 → (128/2) - 64 = 0° (TDC)
        assert!((decode_timing_advance(&[0x80]).unwrap()).abs() < 0.01);
        // 0xA0 = 160 → 80 - 64 = 16° BTDC
        assert!((decode_timing_advance(&[0xA0]).unwrap() - 16.0).abs() < 0.01);
    }

    #[test]
    fn test_stft() {
        // 0x80 = 128 → (128-128)/1.28 = 0.0%
        assert!((decode_stft_bank1(&[0x80]).unwrap()).abs() < 0.01);
    }

    #[test]
    fn test_fuel_level() {
        // 0xFF = 255 → 255/2.55 = 100%
        let v = decode_fuel_level(&[0xFF]).unwrap();
        assert!((v - 100.0).abs() < 0.1, "Fuel level = {v}");
    }

    #[test]
    fn test_maf_hi() {
        // 0x0200 = 512 raw → 512 * (512/65536) = 4.0 g/s
        let raw = [0x02, 0x00];
        let v = decode_maf_hi(&raw).unwrap();
        assert!((v - 4.0).abs() < 0.001, "MAF = {v}");
    }

    #[test]
    fn test_inj_pw() {
        // 0xFFFF = 65535 → 65535/65.535 ≈ 999.985 ms
        let raw = [0xFF, 0xFF];
        let v = decode_inj_pw_bank1(&raw).unwrap();
        assert!(v > 999.0 && v < 1001.0, "Injector PW = {v}");
    }

    #[test]
    fn test_knock_retard() {
        // 0x00 → 0 * 0.0879 = 0°
        assert!((decode_knock_retard(&[0x00]).unwrap()).abs() < 0.001);
        // 0xFF = 255 → 255 * 0.0879 = 22.42°
        let v = decode_knock_retard(&[0xFF]).unwrap();
        assert!((v - 22.41).abs() < 0.1, "Knock retard = {v}");
    }

    #[test]
    fn test_torque() {
        // 0x0064 = 100 raw → 100 * 0.02646484375 = 2.646 N·m
        let raw = [0x00, 0x64];
        let v = decode_torque_request(&raw).unwrap();
        assert!((v - 2.646).abs() < 0.01, "Torque = {v}");
    }

    #[test]
    fn test_dispatch() {
        let r = decode_pid(0x000C, &[0x1B, 0x58]);
        assert_eq!(r.name, "Engine RPM");
        assert!((r.value.unwrap() - 1750.0).abs() < 0.1);
    }

    #[test]
    fn test_pid1105_flags() {
        // bit 0 set = closed loop, bit 5 set = fuel trim learn active
        let flags = decode_pid1105(&[0b00100001]).unwrap();
        assert!(flags.closed_loop);
        assert!(flags.fuel_trim_learn);
        assert!(!flags.decel);
    }

    #[test]
    fn test_empty_raw_returns_none() {
        assert!(decode_engine_rpm(&[]).is_none());
        assert!(decode_ect(&[]).is_none());
        assert!(decode_maf_hi(&[0x01]).is_none()); // needs 2 bytes
    }
}
