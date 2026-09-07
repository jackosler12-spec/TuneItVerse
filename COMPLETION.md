# TuneItVerse v3.10.0 — honest live data, fail-closed flash family, visual UI (2026-09-07)

**Status: v3.9.1 docs said the logger no longer invented RPM. On that tree, `logging.rs::capture_sample` still filled every enabled channel from `simulate_values` before overlaying live PIDs. Missing Mode 01 data therefore stayed fake.**

## What was actually broken

1. Logger stored simulated RPM/MAP/ECT/… whenever a PID did not decode.
2. Diesel template enabled `rail` / `iq` / `vgt` with PID `0x0000` (never decoded).
3. Guided flash and BIN-to-ECU compare defaulted unknown images to `P01_0411`.
4. EDC17 / MED17 / generic Bosch starters were sent as unlock keys.
5. Auto-detect labelled any adapter noise as VPW and kept the port “connected”.
6. Frontend mocks invented VIN, RPM, COM ports, and a successful flash when Tauri was missing.
7. Demo Tables fabricated an 8×8 map and a 512 KB zero image.
8. Table extract failure filled cells with `50`. Hex dump started at hardcoded `0x20000`.
9. Scripts tab called `get_logging_templates` instead of `list_script_helpers`.
10. J2534 radio still opened serial `connect_ecu`.
11. Eight overlay scripts (`v29.js`–`v380.js`) wrapped `invokeCmd`. HTML did not use the sidebar/KPI layout already in `styles.css`. Version strings disagreed (3.9.0 vs 3.9.1).

## What this pass actually changed

1. `capture_sample` stores only live overrides (or imported CSV). `simulate_values` is gone.
2. Diesel / boost templates use Mode 01 PIDs only.
3. `resolved_family` refuses Honda-blocked and size-collision images. Flash/compare never default to P01.
4. `bosch_key_result` marks EDC16C41 4-byte as verified. Unverified families are labelled on the bench and refused on unlock/flash.
5. Auto-detect requires an ELM identity + Mode 01, or a VPW Mode 01 header. Silence is an error; the port is not kept connected.
6. Browser-without-Tauri throws. No mock COM list, VIN, RPM, or flash success.
7. Overlay JS folded into `src/main.js`. Sidebar + KPI + pipeline UI uses the existing design tokens. Connection pulse, view fade, map heat colors, and flash step chips are driven by real state.
8. Versions 3.10.0 across package, crate, Tauri window, HTML, workspace export.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key and will not unlock.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.
6. Honda vs P01: always confirm OS string. The guard is string-based; a stripped dump with no 37820 ASCII can still look like P01 — and 512 KB without an OS string will not flash.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
