# TuneItVerse v3.3.0 — Operational closeout (2026-09-01)

**Status: claimed-but-missing v3.2.1 surfaces are now actually wired.**

## What this pass actually changed

1. `log_import_csv` exists in `logging.rs`, is registered in `lib.rs`, and the Data Logging Import CSV button can reload a previous session for map-from-log.
2. `compute_seed_key` command + Connect-tab calculator. P01/P59 uses the LFSR; EDC16C41 uses the 4-byte RE algorithm with known test vectors.
3. Bosch UDS unlock offline path is **fail-closed** (`success: false`) in Rust and in the browser mock. Use the calculator for bench math.
4. J2534 device list walks `HKLM\SOFTWARE\PassThruSupport.04.04` (+ WOW6432Node) via `reg query` on Windows. Empty registry falls back to the vendor hint list.
5. Map-from-log now reports STFT/LTFT/AFR averages and the top dwell cells. Still hint-only — no auto-write.
6. Logging channel set includes MAF, VSS, load, AFR.
7. Python helper: `python3 python/ecu_scripting.py seedkey EDC16C41 12345678`.

## Still needs your bench

1. EDC17 / MED17 seed tables from *your* dumps (starters in `security.rs` are not claimed as exact).
2. PCM Hammer comparison on your 512 KB P01 OS.
3. Windows box + vendor J2534 DLL to confirm the registry walk against a live driver.
4. Live flash on a spare ECU with verified backup and >=12.5 V.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
