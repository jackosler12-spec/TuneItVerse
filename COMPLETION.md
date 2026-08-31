# TuneItVerse v3.2.1 — Honest operational pass (2026-08-31)

**Status: the v3.1.0 bugs listed on main are closed on this branch.**

## What this pass actually changed

1. `checksum.rs` accepts **128 KB and 512 KB** P01 images (2 or 8 × 64 KB sum-to-zero blocks). Real LS1 dumps are 524288 bytes. EDC16 2 MB CRC32 path unchanged.
2. `GuidedFlashRequest.user_confirmed_risks` defaults to **false** (fail-closed).
3. Guided flash auto-correct runs on 128 KB / 512 KB / 2 MB and **writes the corrected bytes** back into the image used for Mode 34/36.
4. Hex dump starts at the **selected table address**, not a hard-coded `0x20000`.
5. BIN load calls `identify_bin_cmd`.
6. Scripts tab lists `list_script_helpers`, not logging templates.
7. Flash UI sends `user_confirmed_risks: true` only after every risk checkbox is ticked.
8. Browser mock flash returns `success: false`.
9. J2534 device list walks `PassThruSupport.04.04` via `reg query` on Windows.
10. Bosch unlock offline path returns `success: false`.
11. CSV import (`log_import_csv`) + offline `compute_seed_key` command.

## Still needs your bench

1. EDC17 / MED17 seed tables from your dumps
2. PCM Hammer comparison on your 512 KB P01 OS (the 8×64 KB additive layout matches community P01 practice; verify CS word locations on your OS)
3. Windows box + vendor J2534 DLL for a live registry walk

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
