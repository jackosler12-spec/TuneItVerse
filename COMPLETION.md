# TuneItVerse v3.4.1 — operational closeout (2026-09-02)

**Status: v3.4.0 landed on main with incomplete wiring. This pass actually loads ME7/Delphi, talks UDS 0x23 over ISO-TP, and exposes the heatmap / workspace UI.**

v3.4.0 claimed live verify + catalog + heatmap. What was actually on main:

- `ecu_database.rs` still only embedded 5 families. `me7_common.json` and `delphi_dcm.json` existed but were never `include_str!`'d.
- `live_verify.rs` sent a raw ALFI payload through the VPW writer instead of `uds::read_memory_by_address`.
- `index.html` was still v3.3.0: no `v340.js`, no workspace button, no unverified-write checkbox.
- Identify dropped OS-string hits and SHA-256 even though COMPLETION said they were there.
- Live log capture dropped STFT / MAF / VSS / load / O2 after the 3.4 squash.

## What this pass actually changed

1. ECU DB loader embeds ME7_COMMON + DELPHI_DCM. 1MB dumps identify as ME7. Table auto-load reads `ignition_timing` / `fuel_ve` / `boost_target`.
2. UDS 0x23 uses `uds::read_memory_by_address` (ISO-TP). VPW Mode 3C and KWP 0x23 unchanged.
3. Identify reports SHA-256 (full / head 4k / tail 4k), OS-string hits, numeric ID candidates.
4. BIN compare reports contiguous diff ranges.
5. Map-from-log occupancy 16×16 + optional STFT averages. UI heatmap overlay in `v340.js`.
6. `export_workspace_cmd` + Export Workspace buttons.
7. Flash UI injects `user_confirmed_risks` from the four boxes and `accept_unverified_write` from the fifth.
8. Live log capture restored: RPM, MAP, ECT, TPS, IAT, spark, batt, STFT, LTFT, MAF, VSS, load, O2 B1S1.

## Still needs your bench

1. EDC17 / MED17 / ME7 seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. Windows + vendor J2534 DLL against the registry walk.
4. Kernel-resident Mode 3C full-image dump. Windowed probes are not a 512 KB / 2 MB backup.
5. ME7 1MB checksum corrector from a personal dump.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
