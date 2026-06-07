//! pid_decode.rs — P01 LS1 PCM / J1850 VPW PID decoder
//!
//! All decode formulas are derived directly from PidParameters-VPW.XML
//! (source-of-truth for GM Gen III V8 PCM on J1850 VPW).
//!
//! Naming convention:
//!   `decode_<name>(raw: &[u8]) -> Option<f32>`  — standard scalar PIDs
//!   `decode_<name>_bit(raw: u8, bit: u8) -> bool` — bitmapped PIDs
//!
//! DataType mapping from XML:
//!   UBYTE  = raw[0]              (1 byte, unsigned)
//!   UWORD  = (raw[0]<<8)|raw[1]  (2 bytes, unsigned big-endian)
//!   SWORD  = raw as i16          (2 bytes, signed big-endian)

use serde::Serialize;

// ─────────────────────────────────────────────────────────────────────────────
// Result types
// ─────────────────────────────────────────────────────────────────────────────

/// Primary telemetry struct — all decoded values for the live dashboard.
/// Fields map 1:1 to the most-used VPW PIDs.
#[derive(Serialize, Clone, Debug, Default)]
pub struct EcuTelemetry {
    // Engine
    pub rpm:          f32,   // PID 0x0C — RPM
    pub engine_load:  f32,   // PID 0x04 — Engine Load %
    pub map_kpa:      f32,   // PID 0x0B — MAP kPa
    pub map_volts:    f32,   // PID 0x1142 — MAP Volts (GM-specific)
    pub iat_c:        f32,   // PID 0x0F — IAT °C
    pub ect_c:        f32,   // PID 0x05 — ECT °C
    pub tps_pct:      f32,   // PID 0x11 — TPS %
    pub vss_kph:      f32,   // PID 0x0D — VSS km/h
    pub maf_gs:       f32,   // PID 0x10 — MAF g/s
    pub maf_hi_gs:    f32,   // PID 0x1140 — MAF high precision g/s
    pub baro_kpa:     f32,   // PID 0x119D — Barometric pressure kPa

    // Fuel
    pub stft_b1_pct:  f32,   // PID 0x06 — STFT left bank %
    pub ltft_b1_pct:  f32,   // PID 0x07 — LTFT left bank %
    pub stft_b2_pct:  f32,   // PID 0x08 — STFT right bank %
    pub ltft_b2_pct:  f32,   // PID 0x09 — LTFT right bank %
    pub fuel_trim_cell: f32, // PID 0x1190 — Fuel trim cell
    pub evap_purge_pct: f32, // PID 0x1170 — EVAP purge solenoid %
    pub target_afr:   f32,   // PID 0x119E — Target A/F ratio
    pub idc_b1_pct:   f32,   // Math: IDC Bank 1 % (computed)
    pub inj_pw_b1_ms: f32,   // PID 0x125A — Injector PW bank 1 ms
    pub inj_pw_b2_ms: f32,   // PID 0x125B — Injector PW bank 2 ms

    // O2 sensors
    pub o2_left_up_v:   f32, // PID 0x14 — O2 left upstream V
    pub o2_left_dn_v:   f32, // PID 0x15 — O2 left downstream V
    pub o2_right_up_v:  f32, // PID 0x18 — O2 right upstream V
    pub o2_right_dn_v:  f32, // PID 0x19 — O2 right downstream V
    pub o2_lf_mv:       f32, // PID 0x1145 — O2 left front mV (GM)
    pub o2_lr_mv:       f32, // PID 0x1146 — O2 left rear mV (GM)
    pub o2_rf_mv:       f32, // PID 0x1148 — O2 right front mV (GM)
    pub o2_rr_mv:       f32, // PID 0x1149 — O2 right rear mV (GM)
    pub wb_afr:         f32, // Serial WB AFR (external wideband)

    // Ignition
    pub spark_adv_deg:  f32, // PID 0x0E — Ignition timing °
    pub knock_retard:   f32, // PID 0x11A6 — Knock retard °

    // IAC
    pub iac_learned:    f32, // PID 0x1176 — Learned IAC position steps
    pub iac_current:    f32, // PID 0x1179 — Current IAC position steps
    pub idle_desired:   f32, // PID 0x1192 — Desired idle RPM

    // Electrical
    pub batt_volt:      f32, // PID 0x1141 — Ignition 1 signal V

    // Engine vitals
    pub oil_press_kpa:  f32, // PID 0x115C — Engine oil pressure kPa
    pub oil_life_pct:   f32, // PID 0x119F — Engine oil life %
    pub engine_run_min: f32, // PID 0x11A1 — Engine run time minutes
    pub cat_temp_c:     f32, // PID 0x11A3 — Calculated cat temp °C
    pub startup_ect_c:  f32, // PID 0x116F — Start-up ECT °C

    // Transmission (RE4R)
    pub trans_oil_temp: f32, // PID 0x19F3 — Trans oil temp raw
    pub trans_ratio:    f32, // PID 0x19F4 — Transmission ratio raw
    pub trans_gear:     f32, // PID 0x19F5 — Transmission gear raw
    pub current_gear:   f32, // PID 0x199A — Current gear

    // Misfire — per cylinder (8-cylinder LS1)
    pub misfire_c1: f32,     // PID 0x11E6
    pub misfire_c2: f32,     // PID 0x11E7
    pub misfire_c3: f32,     // PID 0x11E8
    pub misfire_c4: f32,     // PID 0x11E9
    pub misfire_c5: f32,     // PID 0x11EA
    pub misfire_c6: f32,     // PID 0x11EB
    pub misfire_c7: f32,     // PID 0x11EC
    pub misfire_c8: f32,     // PID 0x11ED

    // Status / bitmapped
    pub mil_on:          bool,
    pub dtc_count:       u8,
    pub loop_closed:     bool,  // PID 0x1105 bit 0
    pub fuel_trim_learn: bool,  // PID 0x1105 bit 5
    pub cold_startup:    bool,  // PID 0x1105 bit 6
    pub decel_active:    bool,  // PID 0x1105 bit 7
    pub tcc_solenoid:    bool,  // PID 0x1102 bit 2
    pub traction_ctrl:   bool,  // PID 0x1102 bit 0
    pub vtd_fuel_dis:    bool,  // PID 0x1102 bit 1
    pub reduced_power:   bool,  // PID 0x1103 bit 7
    pub mil_cmd:         bool,  // PID 0x1103 bit 1
    pub oil_level_low:   bool,  // PID 0x1103 bit 5
    pub cruise_active:   bool,  // PID 0x1104 bit 4
    pub dtc_set_this_ign:bool,  // PID 0x1104 bit 2
    pub ac_relay:        bool,  // PID 0x1100 bit 3
    pub ac_request:      bool,  // PID 0x1100 bit 0
    pub ac_clutch:       bool,  // PID 0x1100 bit 1
    pub pcm_reset:       bool,  // PID 0x1100 bit 2
    pub evap_vent:       bool,  // PID 0x1101 bit 1
    pub tac_comm_good:   bool,  // PID 0x1115 bit 7
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

#[inline]
fn ubyte(raw: &[u8]) -> Option<f32> {
    raw.first().map(|&b| b as f32)
}

#[inline]
fn uword(raw: &[u8]) -> Option<f32> {
    if raw.len() < 2 {
        return None;
    }
    Some((raw[0] as u16 * 256 + raw[1] as u16) as f32)
}

#[inline]
fn sword(raw: &[u8]) -> Option<f32> {
    if raw.len() < 2 {
        return None;
    }
    Some(((raw[0] as i16) << 8 | raw[1] as i16) as f32)
}

/// Extract a single bit from a byte.
#[inline]
pub fn bit(byte: u8, index: u8) -> bool {
    (byte >> index) & 1 != 0
}

// ─────────────────────────────────────────────────────────────────────────────
// Standard OBD-II / Mode 01 PIDs  (service 0x01 frame[4] == pid)
// ─────────────────────────────────────────────────────────────────────────────

/// PID 0x01 — Monitor status (MIL + DTC count)
pub fn decode_monitor_status(raw: &[u8]) -> Option<(bool, u8)> {
    let b = *raw.first()?;
    Some(((b & 0x80) != 0, b & 0x7F))
}

/// PID 0x04 — Engine Load %  formula: x/2.55
pub fn decode_engine_load(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x05 — ECT °C  formula: x-40
pub fn decode_ect(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x05 — ECT °F
pub fn decode_ect_f(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 40.0) * 1.8 + 32.0)
}

/// PID 0x06 — STFT left bank %  formula: (x-128)/1.28
pub fn decode_stft_b1(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x07 — LTFT left bank %  formula: (x-128)/1.28
pub fn decode_ltft_b1(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x08 — STFT right bank %  formula: (x-128)/1.28
pub fn decode_stft_b2(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x09 — LTFT right bank %  formula: (x-128)/1.28
pub fn decode_ltft_b2(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 128.0) / 1.28)
}

/// PID 0x0B — MAP kPa  formula: x  (raw byte == kPa)
pub fn decode_map(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x0C — Engine Speed RPM  formula: UWORD * 0.25
pub fn decode_rpm(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? * 0.25)
}

/// PID 0x0D — VSS km/h  formula: x
pub fn decode_vss(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x0D — VSS mph  formula: x*0.60934
pub fn decode_vss_mph(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 0.60934)
}

/// PID 0x0E — Ignition timing °  formula: (x/2)-64
pub fn decode_spark_adv(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.0 - 64.0)
}

/// PID 0x0F — IAT °C  formula: x-40
pub fn decode_iat(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x0F — IAT °F
pub fn decode_iat_f(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? - 40.0) * 1.8 + 32.0)
}

/// PID 0x10 — MAF g/s  formula: UWORD/100
pub fn decode_maf(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 100.0)
}

/// PID 0x11 — TPS %  formula: (x*100)/255
pub fn decode_tps(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? * 100.0) / 255.0)
}

/// PID 0x14 — O2 left upstream Volts  (SWORD, raw == volts for GM)
pub fn decode_o2_left_up(raw: &[u8]) -> Option<f32> {
    sword(raw)
}

/// PID 0x15 — O2 left downstream Volts
pub fn decode_o2_left_dn(raw: &[u8]) -> Option<f32> {
    sword(raw)
}

/// PID 0x18 — O2 right upstream Volts
pub fn decode_o2_right_up(raw: &[u8]) -> Option<f32> {
    sword(raw)
}

/// PID 0x19 — O2 right downstream Volts
pub fn decode_o2_right_dn(raw: &[u8]) -> Option<f32> {
    sword(raw)
}

// ─────────────────────────────────────────────────────────────────────────────
// GM-specific Mode 22 PIDs  (service 0x22, 2-byte PID in frame[4..5])
// These use the 0x11xx / 0x12xx / 0x19xx / 0xFCxx PID range.
// ─────────────────────────────────────────────────────────────────────────────

// ── 0x1100 byte — bitmapped switches ──────────────────────────────────────

/// PID 0x1100 bit 3 — A/C Relay Command
pub fn decode_ac_relay(raw: u8) -> bool { bit(raw, 3) }
/// PID 0x1100 bit 0 — A/C Request Signal
pub fn decode_ac_request(raw: u8) -> bool { bit(raw, 0) }
/// PID 0x1100 bit 1 — A/C Clutch Feedback
pub fn decode_ac_clutch(raw: u8) -> bool { bit(raw, 1) }
/// PID 0x1100 bit 2 — PCM Reset
pub fn decode_pcm_reset(raw: u8) -> bool { bit(raw, 2) }
/// PID 0x1100 bit 7 — GEN L-Terminal Signal Command
pub fn decode_gen_l_term(raw: u8) -> bool { bit(raw, 7) }

// ── 0x1101 ────────────────────────────────────────────────────────────────
/// PID 0x1101 bit 1 — EVAP Vent Solenoid Command
pub fn decode_evap_vent(raw: u8) -> bool { bit(raw, 1) }

// ── 0x1102 ────────────────────────────────────────────────────────────────
/// PID 0x1102 bit 1 — VTD Fuel Disable
pub fn decode_vtd_fuel_disable(raw: u8) -> bool { bit(raw, 1) }
/// PID 0x1102 bit 2 — TCC Enable Solenoid
pub fn decode_tcc_solenoid(raw: u8) -> bool { bit(raw, 2) }
/// PID 0x1102 bit 0 — Traction Control Status
pub fn decode_traction_ctrl(raw: u8) -> bool { bit(raw, 0) }
/// PID 0x1102 bit 5 — Powertrain Induced Chassis Pitch
pub fn decode_pitch_cmd(raw: u8) -> bool { bit(raw, 5) }
/// PID 0x1102 bit 6 — Extended Travel Brake Pedal Switch
pub fn decode_ext_brake(raw: u8) -> bool { bit(raw, 6) }
/// PID 0x1102 bit 3 — TCC/Cruise Brake Pedal Switch
pub fn decode_tcc_brake(raw: u8) -> bool { bit(raw, 3) }

// ── 0x1103 ────────────────────────────────────────────────────────────────
/// PID 0x1103 bit 7 — Reduced Engine Power
pub fn decode_reduced_power(raw: u8) -> bool { bit(raw, 7) }
/// PID 0x1103 bit 5 — Engine Oil Level Switch (Low)
pub fn decode_oil_level_low(raw: u8) -> bool { bit(raw, 5) }
/// PID 0x1103 bit 1 — MIL Command
pub fn decode_mil_cmd(raw: u8) -> bool { bit(raw, 1) }

// ── 0x1104 ────────────────────────────────────────────────────────────────
/// PID 0x1104 bit 4 — Cruise Control Active
pub fn decode_cruise_active(raw: u8) -> bool { bit(raw, 4) }
/// PID 0x1104 bit 2 — DTC Set This Ignition
pub fn decode_dtc_set_this_ign(raw: u8) -> bool { bit(raw, 2) }
/// PID 0x1104 bit 1 — AIR Solenoid Command
pub fn decode_air_sol(raw: u8) -> bool { bit(raw, 1) }
/// PID 0x1104 bit 0 — AIR Pump Relay Command
pub fn decode_air_pump(raw: u8) -> bool { bit(raw, 0) }

// ── 0x1105 ────────────────────────────────────────────────────────────────
/// PID 0x1105 bit 0 — Loop Status (Closed=true)
pub fn decode_loop_closed(raw: u8) -> bool { bit(raw, 0) }
/// PID 0x1105 bit 7 — Decel Active
pub fn decode_decel(raw: u8) -> bool { bit(raw, 7) }
/// PID 0x1105 bit 5 — Fuel Trim Learn Active
pub fn decode_fuel_trim_learn(raw: u8) -> bool { bit(raw, 5) }
/// PID 0x1105 bit 6 — Cold Startup
pub fn decode_cold_startup(raw: u8) -> bool { bit(raw, 6) }

// ── 0x1106/0x1107 ─────────────────────────────────────────────────────────
/// PID 0x1106 bit 0 — FC Relay 1
pub fn decode_fc_relay1(raw: u8) -> bool { bit(raw, 0) }
/// PID 0x1106 bit 1 — FC Relay 2 and 3
pub fn decode_fc_relay23(raw: u8) -> bool { bit(raw, 1) }

// ── 0x1115/0x1116 ─────────────────────────────────────────────────────────
/// PID 0x1115 bit 7 — TAC/PCM Communication (Good=true)
pub fn decode_tac_comm(raw: u8) -> bool { bit(raw, 7) }
/// PID 0x1116 bit 6 — Cruise Resume/Accel
pub fn decode_cruise_resume(raw: u8) -> bool { bit(raw, 6) }
/// PID 0x1116 bit 5 — Cruise Set/Coast
pub fn decode_cruise_set(raw: u8) -> bool { bit(raw, 5) }
/// PID 0x1116 bit 4 — Cruise On/Off
pub fn decode_cruise_onoff(raw: u8) -> bool { bit(raw, 4) }
/// PID 0x1116 bit 7 — Stoplamp Pedal Switch
pub fn decode_stoplamp(raw: u8) -> bool { bit(raw, 7) }

// ── Scalar GM PIDs ─────────────────────────────────────────────────────────

/// PID 0x1140 — MAF high precision g/sec  formula: UWORD*(512/65536)
pub fn decode_maf_hi(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? * (512.0 / 65536.0))
}

/// PID 0x1141 — Ignition 1 Signal Volts  formula: x/10
pub fn decode_batt_volt(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 10.0)
}

/// PID 0x1142 — MAP Sensor Volts  formula: x/51
pub fn decode_map_volts(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1143 — TP Sensor Volts  formula: x/51
pub fn decode_tp_volts(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1144 — A/C Wideband AFR  formula: ((x/51)/.5)+9.58
pub fn decode_ac_wb_afr(raw: &[u8]) -> Option<f32> {
    Some(((ubyte(raw)? / 51.0) / 0.5) + 9.58)
}

/// PID 0x1144 — A/C High Side Pressure kPa  formula: ((x*1.833)-14.95)*6.895
pub fn decode_ac_press_kpa(raw: &[u8]) -> Option<f32> {
    Some(((ubyte(raw)? * 1.833) - 14.95) * 6.895)
}

/// PID 0x1145 — O2 Left Front mV  formula: x/0.2304
pub fn decode_o2_lf_mv(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x1146 — O2 Left Rear mV  formula: x/0.2304
pub fn decode_o2_lr_mv(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x1148 — O2 Right Front mV  formula: x/0.2304
pub fn decode_o2_rf_mv(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x1149 — O2 Right Rear mV  formula: x/0.2304
pub fn decode_o2_rr_mv(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 0.2304)
}

/// PID 0x114B — EGR Wideband AFR  formula: ((x/51)/.5)+9.58
pub fn decode_egr_wb_afr(raw: &[u8]) -> Option<f32> {
    Some(((ubyte(raw)? / 51.0) / 0.5) + 9.58)
}

/// PID 0x114D — GEN F-Terminal Signal %  formula: x/2.55
pub fn decode_gen_f_term(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x114E — Fuel Tank Pressure Sensor Volts  formula: x/51
pub fn decode_fuel_tank_press_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x1151 — Accelerator Pedal %  formula: x*(100/256)
pub fn decode_accel_pedal(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * (100.0 / 256.0))
}

/// PID 0x1155 — Fuel Level Sensor Volts  formula: x/51
pub fn decode_fuel_level_v(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 51.0)
}

/// PID 0x115C — Engine Oil Pressure kPa  formula: (x*4.34)-104.2
pub fn decode_oil_press_kpa(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? * 4.34) - 104.2)
}

/// PID 0x116F — Start-Up ECT °C  formula: x-40
pub fn decode_startup_ect(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? - 40.0)
}

/// PID 0x1170 — EVAP Purge Solenoid %  formula: x/2.55
pub fn decode_evap_purge(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x1171 — EGR Duty Cycle %  formula: x/2.55
pub fn decode_egr_duty(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55)
}

/// PID 0x1176 — Learned IAC Position steps  formula: x
pub fn decode_iac_learned(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1179 — Current IAC Position steps  formula: x
pub fn decode_iac_current(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1190 — Fuel Trim Cell  formula: x
pub fn decode_fuel_trim_cell(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x1192 — Desired Idle Speed RPM  formula: x*12.5
pub fn decode_desired_idle(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 12.5)
}

/// PID 0x119D — Barometric Pressure (sampled) kPa  formula: (x+28)/2.71
pub fn decode_baro(raw: &[u8]) -> Option<f32> {
    Some((ubyte(raw)? + 28.0) / 2.71)
}

/// PID 0x119E — Target A/F Ratio :1  formula: x/10
pub fn decode_target_afr(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 10.0)
}

/// PID 0x119F — Engine Oil Life %  formula: x/2.55+0.5
pub fn decode_oil_life(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? / 2.55 + 0.5)
}

/// PID 0x11A1 — Engine Run Time minutes  formula: UWORD/60
pub fn decode_run_time_min(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 60.0)
}

/// PID 0x11A3 — Calculated Cat Temp °C  formula: UBYTE*2050/256
pub fn decode_cat_temp(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 2050.0 / 256.0)
}

/// PID 0x11A6 — Knock Retard °  formula: x*0.0879
pub fn decode_knock_retard(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * 0.0879)
}

// ── Misfire — per cylinder (PIDs 0x11E6..0x11ED current, 0x11F0..0x11F7 history) ──
/// PIDs 0x11E6–0x11ED — Misfire current count, cylinder 1–8  formula: x
pub fn decode_misfire(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ── Injector pulse width (GM Mode 22) ─────────────────────────────────────

/// PID 0x125A — Injector PW Bank 1 ms (if present, UWORD * 0.003906 from original lib)
pub fn decode_inj_pw(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? * 0.003906)
}

// ── Transmission ──────────────────────────────────────────────────────────

/// PID 0x19F3 — Trans Oil Temp (raw byte — no scale defined in XML)
pub fn decode_trans_oil_temp(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

/// PID 0x19F4 — Transmission Ratio (UWORD raw)
pub fn decode_trans_ratio(raw: &[u8]) -> Option<f32> {
    uword(raw)
}

/// PID 0x19F5 — Transmission Gear (UWORD raw)
pub fn decode_trans_gear(raw: &[u8]) -> Option<f32> {
    uword(raw)
}

/// PID 0x199A — Current Gear (UBYTE raw)
pub fn decode_current_gear(raw: &[u8]) -> Option<f32> {
    ubyte(raw)
}

// ── Math / derived channels ────────────────────────────────────────────────

/// Math: Load from RPM and MAF  formula: maf_gs * 15.0 / (rpm * 2)
/// Per XML: g/cyl = x*15.0/(y*2)  where x=MAF g/s, y=RPM
pub fn calc_load_gcyl(maf_gs: f32, rpm: f32) -> Option<f32> {
    if rpm < 1.0 {
        return None;
    }
    Some(maf_gs * 15.0 / (rpm * 2.0))
}

/// Math: IDC Bank 1 %  formula: (rpm * inj_pw_ms) / 1200
pub fn calc_idc_b1(rpm: f32, inj_pw_ms: f32) -> f32 {
    (rpm * inj_pw_ms) / 1200.0
}

/// Math: IDC Bank 2 %  formula: (rpm * inj_pw_ms) / 1200
pub fn calc_idc_b2(rpm: f32, inj_pw_ms: f32) -> f32 {
    (rpm * inj_pw_ms) / 1200.0
}

/// FC05 — Target Equivalence Ratio  formula: UWORD/1024
pub fn decode_target_eqr(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 1024.0)
}

/// FC05 — EGR PWM Duty Cycle %  formula: x*(100/256)
pub fn decode_egr_pwm(raw: &[u8]) -> Option<f32> {
    Some(ubyte(raw)? * (100.0 / 256.0))
}

// ─────────────────────────────────────────────────────────────────────────────
// RAM Address parameters (Mode 23 / direct RAM reads from OS 12593358)
// These are read by address, not PID — listed here for completeness.
// ─────────────────────────────────────────────────────────────────────────────
//
// | Parameter                    | Address  | Formula            | Units  |
// |------------------------------|----------|--------------------|--------|
// | Load                         | 0xFFAAEA | UWORD/2048.0       | g/cyl  |
// | On Time                      | 0xFFB04C | UWORD/160.0        | Sec    |
// | Throttle Cracker Airflow     | 0xFF97F0 | UWORD/1024.0       | g/s    |
// | Throttle Follower Airflow    | 0xFF980A | UWORD/1024.0       | g/s    |
// | Idle Proportional Term       | 0xFFA2A2 | UWORD/16.0         | g/s    |
// | Idle Integral Term           | 0xFFA296 | UWORD/16.0         | g/s    |
// | Idle Derivative Term         | 0xFFA28C | UWORD/16.0         | g/s    |
// | Ignition Advance Multiplier  | 0xFF8250 | UWORD/4096.0       | Factor |
// | Manifold Abs Pressure        | 0xFFB292 | UWORD/51.2         | kPa    |

/// RAM Load  formula: UWORD/2048.0  (OS 12593358: addr 0xFFAAEA)
pub fn decode_ram_load(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 2048.0)
}

/// RAM On Time (injector)  formula: UWORD/160.0
pub fn decode_ram_on_time(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 160.0)
}

/// RAM MAP (2-byte)  formula: UWORD/51.2  — higher precision than PID 0x0B
pub fn decode_ram_map(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 51.2)
}

/// RAM airflow (throttle cracker / follower / idle PID/I/D)  formula: UWORD/1024.0
pub fn decode_ram_airflow(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 1024.0)
}

/// RAM ignition advance multiplier  formula: UWORD/4096.0
pub fn decode_ram_ign_mult(raw: &[u8]) -> Option<f32> {
    Some(uword(raw)? / 4096.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpm() {
        // 3000 RPM: raw UWORD = 3000/0.25 = 12000 = 0x2EE0
        assert_eq!(decode_rpm(&[0x2E, 0xE0]), Some(3000.0));
    }

    #[test]
    fn test_ect() {
        // 90°C: raw = 90+40 = 130 = 0x82
        assert_eq!(decode_ect(&[0x82]), Some(90.0));
    }

    #[test]
    fn test_stft() {
        // 0% trim: raw = 128
        let v = decode_stft_b1(&[128]).unwrap();
        assert!((v).abs() < 0.01, "Expected ~0.0, got {}", v);
    }

    #[test]
    fn test_spark() {
        // 10° BTDC: raw = (10+64)*2 = 148
        assert_eq!(decode_spark_adv(&[148]), Some(10.0));
    }

    #[test]
    fn test_tps() {
        // 50%: raw = 255*0.5 ≈ 128
        let v = decode_tps(&[128]).unwrap();
        assert!((v - 50.196).abs() < 0.01, "Expected ~50.2, got {}", v);
    }

    #[test]
    fn test_batt_volt() {
        // 14.2V: raw = 142
        assert_eq!(decode_batt_volt(&[142]), Some(14.2));
    }

    #[test]
    fn test_maf() {
        // 100 g/s: UWORD = 10000 = 0x2710
        assert_eq!(decode_maf(&[0x27, 0x10]), Some(100.0));
    }

    #[test]
    fn test_knock_retard() {
        // 5° retard: raw = 5/0.0879 ≈ 56.9 ≈ 57
        let v = decode_knock_retard(&[57]).unwrap();
        assert!((v - 5.0103).abs() < 0.01, "Expected ~5.0, got {}", v);
    }

    #[test]
    fn test_bits() {
        // TCC solenoid on: byte = 0x04 (bit 2)
        assert!(decode_tcc_solenoid(0x04));
        assert!(!decode_tcc_solenoid(0x00));
    }
}
