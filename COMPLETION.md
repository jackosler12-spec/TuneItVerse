# TuneItVerse v3.9.0 — wire the features v3.8 claimed (2026-09-06)

**Status: v3.8.0 added `cs_guard.rs` and `v380.js` but never compiled them in. `read_ecu_data` still seeded demo RPM. This pass fixes the wiring.**

## What was actually broken on main

1. `mod cs_guard` and `mod checksum_sizes` were missing from `lib.rs`. The Honda guard module could not compile into the binary.
2. `scan_checksum_candidates_cmd` and `import_workspace_cmd` were not in `generate_handler`.
3. `src/index.html` never loaded `v380.js`.
4. `read_ecu_data` defaulted RPM/MAP/ECT to demo numbers and labelled offline as `offline-demo`.
5. `identify_bin` picked the first same-size family (P01) on every 512KB image, including Honda dumps.
6. `correct_checksums` would still run P01 additive on a Honda-sized image.

## What this pass actually changed

1. Register `checksum_sizes` + `cs_guard`. Expose `scan_checksum_candidates_cmd` and `import_workspace_cmd`.
2. Load `v380.js` from `index.html`.
3. `read_ecu_data` only returns PIDs that decoded. Offline is `{source:"offline", pids_decoded:0}`.
4. Identify: size collision leaves `family` unset unless an OS string hits. Honda OS (`37820*`, KEIHIN, K20A/K24A) selects `HONDA_KEIHIN` and sets `correction_safe=false`.
5. Checksum validate on Honda-blocked images is report-only `HONDA_KEIHIN`. Correct errors instead of rewriting the dump.
6. Connect path sends a short ELM AT warmup (SP2/SP5/SP6 by protocol). Disconnect clears `last_os_id`.
7. Versions 3.9.0.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.
6. Honda vs P01: always confirm OS string. The guard is string-based; a stripped dump with no 37820 ASCII can still look like P01.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
