# TuneItVerse v2.9.0 — Industry-Leading DIY Platform (2026-08-27)

**Status: v2.9.0 OPERATIONAL** — Aggressive analysis found *real* gaps under the v2.8.0 “fully done” claim. Those gaps are closed here.

## What this pass actually fixed

- **P01 checksum now accepts 512 KB dumps** (the real LS1 / P01 size). v2.8.0 only accepted 128 KB, so every stock 512 KB bin failed validation.
- **`read_properties` no longer hardcodes OS `12225074`.** Live path queries Mode 09 VIN + Mode 01 PID 0x00. Offline reports UNREAD instead of a fake Holden OS.
- **BIN identify** scans the image for known OS / part numbers and size-matches the ECU DB.
- **BIN vs BIN compare** (stock vs tuned) with first-diff list and percent changed.
- **Map-from-log** summarises a logging session and hints the VE-style cell you spent time in.
- **Guided flash offline path is fail-closed.** It will not report `success: true` when you are not connected.
- **Live PIDs expanded:** STFT, LTFT, MAF, VSS, engine load feed both the live panel and the logger.
- **Hex editor jumps to the selected table address.** Checksum summary is shown in the side panel, not swallowed.
- UI / package / Cargo / tauri.conf synchronized to **2.9.0** (the UI was still advertising 2.7.0).

## What already worked (kept)

Serial connect, DTC 03/07/0A + freeze + clear, live Mode 01 core PIDs, logging engine + CSV, XDF parse/patch, guided flash with voltage gate + mid-transfer recheck when *connected*, J2534 surface, Bosch/GM security modules, 5-family ECU DB.

## Still optional / needs your bench

1. Exact EDC17 / MED17 seed tables from *your* dumps
2. Embedded Python runtime (python/ecu_scripting.py is still offline)
3. Windows registry J2534 enumeration
4. Full tokio async I/O on multi-minute transfers
5. Hardware validation of 512 KB additive checksum vs PCM Hammer on *your* specific OS

## Safety

Never flash without a verified backup and stable power. Personal dumps only. This tool is free and honest about what it can and cannot prove without hardware.

Build your own. No bullshit prices.
