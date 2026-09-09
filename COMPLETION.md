# TuneItVerse v3.12.0 — catalog, checksum UI, mid-UDS voltage (2026-09-09)

v3.11.0 made live I/O protocol-aware. The HTML/footer still said 3.10.2, the dashboard did not render the ECU catalog the docs claimed, Tables had no Validate/Correct checksum buttons, and Flash had no standalone voltage or unlock controls. UDS 0x34/36/37 also skipped the mid-transfer voltage abort that VPW already had.

## What this pass actually changed

1. Version 3.12.0 across package, crate, Tauri window, installer, HTML, workspace export, and UI.
2. Dashboard renders `list_ecu_catalog` and live VIN/CALID from `read_properties` when connected.
3. Tables: Validate checksums + Correct checksums (fail-closed Honda / unknown size).
4. Flash: Check battery voltage, Unlock L1/L2, Bosch UDS unlock — wired to existing Rust commands.
5. UDS `download_image` aborts if a J2534 `READ_VBATT` sample drops below 12.5 V every 4 KB.
6. `app_info`, `read_battery_voltage_cmd`, `correct_bin_checksums_report` commands.
7. Map-from-log also reports mean LTFT and engine load.
8. Last serial port / baud / protocol restored from localStorage.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key and will not unlock.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
