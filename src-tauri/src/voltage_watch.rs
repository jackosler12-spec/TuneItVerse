//! Adapter-side supply monitoring. Never sends a diagnostic request during a write.
//! J2534 Vbatt is an IOCTL on the pass-thru. Serial PID 0x42 / AT RV is pre/post only
//! unless the session is not in binary programming mode (ELM AT RV is safe outside binary).
//!
//! For serial mid-transfer monitoring: poll `AT RV` between chunks when the adapter is in
//! text mode and no binary programming session is active. Abort on sag before the next chunk.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyClass {
    Ok,
    Warn,
    Abort,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VoltageWatch {
    pub samples: u32,
    pub min_seen: Option<f32>,
    pub warns: u32,
}

impl VoltageWatch {
    pub fn observe(&mut self, volts: f32) {
        if volts <= 0.0 {
            return;
        }
        self.samples = self.samples.saturating_add(1);
        self.min_seen = Some(self.min_seen.map(|m| m.min(volts)).unwrap_or(volts));
    }

    /// Record a warning if the sample is in the warn band.
    pub fn observe_and_classify(&mut self, volts: f32, min_v: f32) -> SupplyClass {
        self.observe(volts);
        let c = classify(volts, min_v);
        if c == SupplyClass::Warn {
            self.warns = self.warns.saturating_add(1);
        }
        c
    }
}

pub fn classify(volts: f32, min_v: f32) -> SupplyClass {
    if volts <= 0.0 {
        return SupplyClass::Ok;
    }
    if volts < min_v {
        SupplyClass::Abort
    } else if volts < min_v + 0.4 {
        SupplyClass::Warn
    } else {
        SupplyClass::Ok
    }
}

/// ELM327 `AT RV` returns adapter pin voltage, e.g. "12.6V" or "12.6".
/// This must not be used inside a binary programming session.
pub fn parse_at_rv(text: &str) -> Option<f32> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("?") || trimmed.contains("ERROR") {
        return None;
    }
    let mut num = String::new();
    let mut seen_dot = false;
    for c in trimmed.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            num.push(c);
        } else if !num.is_empty() {
            break;
        }
    }
    let v = num.parse::<f32>().ok()?;
    if (8.0..=18.0).contains(&v) { Some(v) } else { None }
}

pub fn sag_message(volts: f32, min_v: f32) -> String {
    format!("Supply sag mid-write: {volts:.2} V (min {min_v:.2}). Transfer aborted before the next chunk.")
}

/// Recommended poll interval for serial AT RV between transfer chunks (when not in binary mode).
pub const SERIAL_RV_POLL_INTERVAL_MS: u64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elm_rv() {
        assert_eq!(parse_at_rv("12.6V\r>"), Some(12.6));
        assert_eq!(parse_at_rv("AT RV\r11.9"), Some(11.9));
        assert!(parse_at_rv("?").is_none());
        assert!(parse_at_rv("NO DATA").is_none());
    }

    #[test]
    fn parses_real_atrv_replies() {
        // Real ATRV replies from the OBDLink MX+ log (Patrol + Chevrolet sessions).
        for v in ["12.5", "12.6", "13.2", "13.4", "13.5", "13.6", "13.7", "13.8", "13.9", "14.0", "14.1", "14.2", "14.3", "14.6", "14.7", "14.8", "14.9"] {
            let want: f32 = v.parse().unwrap();
            assert_eq!(parse_at_rv(&format!("{v}V")), Some(want), "{v}");
            assert_eq!(parse_at_rv(&format!("{v}V\r\r>")), Some(want), "{v} with prompt");
        }
        // STVR prints the number without 'V'.
        assert_eq!(parse_at_rv("14.1\r>"), Some(14.1));
        assert!(parse_at_rv("NO DATA\r>").is_none());
        assert!(parse_at_rv("?\r>").is_none());
    }

    #[test]
    fn classifies_band() {
        // Warn band is 0.4 V above min_v (classify's conservative margin).
        assert_eq!(classify(12.9, 12.5), SupplyClass::Ok);
        assert_eq!(classify(13.2, 12.5), SupplyClass::Ok);
        assert_eq!(classify(12.6, 12.5), SupplyClass::Warn);
        assert_eq!(classify(12.55, 12.5), SupplyClass::Warn);
        assert_eq!(classify(12.4, 12.5), SupplyClass::Abort);
        assert_eq!(classify(0.0, 12.5), SupplyClass::Ok);
    }

    #[test]
    fn watch_tracks_min() {
        let mut w = VoltageWatch::default();
        w.observe(13.1);
        w.observe(12.4);
        w.observe(0.0);
        assert_eq!(w.samples, 2);
        assert_eq!(w.min_seen, Some(12.4));
    }

    #[test]
    fn observe_and_classify_counts_warns() {
        let mut w = VoltageWatch::default();
        assert_eq!(w.observe_and_classify(12.6, 12.5), SupplyClass::Warn);
        assert_eq!(w.warns, 1);
        assert_eq!(w.observe_and_classify(13.0, 12.5), SupplyClass::Ok);
        assert_eq!(w.warns, 1);
    }
}
