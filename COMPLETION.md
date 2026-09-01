# TuneItVerse v3.3.0 — wiring pass (2026-09-01)

**Status: the features COMPLETION v3.2.1 advertised are now actually registered and fail-closed.**

This pass does not invent new ECU families or claim live-bench coverage you have not verified. It closes the holes that made the last "complete" release lie to the UI.

## What this pass actually changed

1. `GuidedFlashRequest.user_confirmed_risks` now defaults to **false**. Omitting the field refuses the write.
2. Flash UI only sends `user_confirmed_risks: true` after every risk checkbox is ticked.
3. `log_import_csv` exists in `logging.rs`, is registered in `lib.rs`, and is wired to the Import CSV button.
4. `compute_seed_key` exists and is registered. Connect tab has an offline seed → key bench (P01/P59 LFSR + Bosch family dispatch).
5. `bosch_uds_unlock` offline / disconnected path returns `success: false`. Browser mock does the same.
6. Connect → J2534 now calls `j2534_connect` / `j2534_connect_vpw` instead of silently opening a serial port.
7. `j2534_list_devices` walks `HKLM\SOFTWARE\PassThruSupport.04.04` (+ Wow6432Node) on Windows via `reg query`. Non-Windows is honest about PassThru being a Windows API.
8. `main.js` exposes `window.invokeCmd` / `window.currentBin` so the v3.2.1 overlay can actually wrap load/identify/hex.
9. Scripts tab lists `list_script_helpers`, not logging templates.
10. Extra log channels: MAF, VSS, load, O2 B1S1. Live capture pulls O2 PID 0x14 when the adapter answers.
11. Python helper: `python3 python/ecu_scripting.py seedkey P01_0411 1234 1`.

## Still needs your bench

1. EDC17 / MED17 seed tables from your dumps — starters in `security.rs` are not claimed as verified.
2. PCM Hammer comparison on your 512 KB P01 OS (8×64 KB additive layout).
3. A Windows box + vendor J2534 DLL to confirm the registry walk against a real driver.
4. Live Mode 23 / Mode 3C bulk backup on the car. Offline flash stays refused.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
