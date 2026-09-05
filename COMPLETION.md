# TuneItVerse v3.8.0 — honest live data + Honda/P01 collision guard (2026-09-05)

**Status: previous passes claimed UDS radio / Honda seed options / "fully operational" while live PIDs still invented demo RPM when a request failed. This pass fixes the lies we can fix in software.**

## What this pass actually changed

1. `read_ecu_data` only returns PIDs that decoded. Offline is `{source:"offline", pids_decoded:0}` — no fake 1250 RPM labelled as live.
2. 512KB identify: if more than one catalog family shares the size, `family` stays unset until an OS string hits. `size_collision` is explicit.
3. Honda OS (`37820*`, KEIHIN, K20A/K24A) on a P01-sized image: checksum **validate** is report-only `HONDA_KEIHIN`, **correct** errors. P01 additive will not silently rewrite a Honda dump.
4. `scan_checksum_candidates` reports 64KB windows whose additive sum16 is already 0. Report-only. No invented corrector.
5. `import_workspace_cmd` accepts exported workspace JSON (metadata only).
6. Connect UI actually has a UDS radio. Seed-key family list includes SID803 and Honda. Versions 3.8.0.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.
6. Honda vs P01: always confirm OS string. The guard is string-based; a stripped dump with no 37820 ASCII can still look like P01.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
