# TuneItVerse v3.13.0 — wire the v3.12 surface that never compiled in (2026-09-10)

v3.12.0 documented dashboard catalog, Validate/Correct checksum buttons, flash voltage/unlock helpers, and mid-UDS voltage abort. On main those extras lived in `v312.rs` / `v312.js` but `mod v312` was missing, the new commands were not in `generate_handler!`, `index.html` never loaded `v312.js`, and the catalog / checksum / voltage buttons were not in the HTML. A desktop build of 3.12 therefore could not invoke the new commands.

## What this pass actually changed

1. Version 3.13.0 across package, crate, Tauri window, installer, HTML, overlay, and docs.
2. `mod v312` + registered `app_info`, `read_battery_voltage_cmd`, `correct_bin_checksums_report`, `session_snapshot`.
3. Dashboard renders `list_ecu_catalog` and VIN/CALID from `read_properties` / `session_snapshot`.
4. Tables: Validate checksums + Correct checksums (fail-closed Honda / unknown size). Wired in both `main.js` and `v312.js`.
5. Flash: Check battery voltage, Unlock L1/L2, Bosch UDS unlock — wired to existing Rust commands.
6. Last serial port / baud / protocol restored from `localStorage` after port refresh.
7. `index.html` loads `v312.js` after `main.js`. Mid-UDS voltage abort from v3.12 remains in the UDS write path.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key and will not unlock.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
