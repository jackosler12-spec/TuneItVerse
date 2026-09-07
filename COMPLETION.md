# TuneItVerse v3.9.1 — claimed v3.9.0 features that were not in the binary (2026-09-07)

**Status: v3.9.0 docs said Honda/P01 collision and workspace import were enforced. On main, `import_workspace_cmd` did not exist (the Tauri handler would not compile) and checksum/identify still treated every 512KB image as P01.**

## What was actually broken on main

1. `v29_tools::import_workspace_cmd` was registered in `generate_handler!` but the function was missing.
2. `identify_bin` still set `family` from the first same-size catalog entry (P01 on every 512KB dump).
3. `validate_checksums` / `correct_checksums` never called `honda_blocks_p01_corrector`.
4. Guided flash used VPW Mode 34/36/37 for Bosch families.
5. `log_capture_sample` invented RPM/MAP when no live Mode 01 data arrived.

## What this pass actually changed

1. Implement `import_workspace_cmd` and keep it registered. Workspace JSON is metadata only — it does not restore BIN bytes.
2. Identify: size collision leaves `family` unset unless an OS string hits. Honda OS (`37820*`, KEIHIN, K20A/K24A) selects `HONDA_KEIHIN` and sets `correction_safe=false`.
3. Checksum validate on Honda-blocked images is report-only `HONDA_KEIHIN`. Correct returns an error instead of rewriting the dump. Guided flash refuses Honda images.
4. Bosch / Delphi / SID families use UDS 0x34/36/37 (`uds::download_image`) after `bosch_uds_unlock_full`. GM P01/P59 stay on VPW Mode 34/36.
5. Logger samples only store live overrides or imported CSV. No invented RPM.
6. Versions 3.9.1. Python CLI identify reports Honda/GM strings.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.
6. Honda vs P01: always confirm OS string. The guard is string-based; a stripped dump with no 37820 ASCII can still look like P01.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
