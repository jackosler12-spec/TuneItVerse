# TuneItVerse v3.0.0 — Honest operational pass (2026-08-28)

**Status: v3.0.0 OPERATIONAL for supported families, with previously advertised v2.9 features actually wired.**

v2.9.0 docs claimed BIN identify, 512 KB P01 checksums, live VIN, expanded PIDs, and fail-closed offline flash. The code on `main` did not match that claim. This pass closes those holes.

## What was actually broken on v2.9.0 main

- `v29_tools.rs` existed but `lib.rs` never declared `mod v29_tools` and never registered `identify_bin_cmd` / `compare_bins_cmd` / `map_from_log_cmd`. The Tables buttons called commands that did not exist.
- P01 checksum validation still required **131072 bytes**. Real LS1 / P01 dumps are **524288 bytes** and were rejected.
- `read_properties` still hardcoded OS `12225074` and returned a fake Holden identity when offline.
- `guided_flash_pipeline` returned `success: true` on the offline path.
- Live Mode 01 never requested STFT / LTFT / MAF / VSS / load even though decoders already existed.
- Hex dump always started at `0x20000` instead of the selected table address.
- Scripts tab listed logging templates instead of script helpers.

## What this pass implements

- Wire v29 tools into the Tauri handler.
- Accept and correct P01 additive checksums on both 128 KB slices and 512 KB dumps (8 × 64 KB blocks).
- Live properties: Mode 09 VIN (0x02) + CALID (0x04) + Mode 01 PID 0x00 mask. Offline reports `UNREAD`, not a fake OS.
- Fail-closed guided flash when not connected.
- Live + logger feed for STFT, LTFT, MAF, VSS, engine load.
- Hex editor jumps to the selected table address. Checksum summary is shown in the side panel.
- Scripts tab lists real helper templates. `python/ecu_scripting.py` is a usable CLI for personal dumps.
- UI sends `user_confirmed_risks` on guided flash.

## Still needs your bench / dumps

1. Exact EDC17 / MED17 seed tables from *your* dumps
2. Embedded Python runtime (PyO3) — intentionally not faked
3. Windows registry J2534 enumeration
4. Full tokio async I/O on multi-minute transfers
5. Hardware validation of 512 KB additive checksum vs PCM Hammer on *your* specific OS

## Safety

Never flash without a verified backup and stable power. Personal dumps only. This tool is free and honest about what it can and cannot prove without hardware.

Build your own. No bullshit prices.
