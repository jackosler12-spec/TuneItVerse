# TuneItVerse v3.1.0 — Honest operational pass (2026-08-29)

**Status: v3.1.0 actually wires the features v3.0.0 documentation claimed.**

v3.0.0 COMPLETION.md said identify / 512 KB P01 checksums / live VIN / fail-closed flash were landed. On `main` they were not:

- `v29_tools.rs` existed but `lib.rs` never declared `mod v29_tools` and never registered the three commands the Tables buttons call.
- P01 checksum still rejected anything except 131072 bytes. Real LS1 dumps are 524288.
- `read_properties` still hardcoded OS `12225074` and invented a Holden identity when offline.
- `guided_flash_pipeline` returned `success: true` on the offline path.
- Live Mode 01 still skipped STFT / LTFT / MAF / VSS / load.
- Hex dump still started at `0x20000`.
- Scripts tab still listed logging templates.
- Pre-flash auto-correct only ran on 128 KB images.
- `user_confirmed_risks` defaulted to `true` if the UI omitted it.

## What this pass implements

- `mod v29_tools` + `identify_bin_cmd` / `compare_bins_cmd` / `map_from_log_cmd` in the Tauri handler.
- P01 additive checksums on 128 KB (2 × 64 KB) **and** 512 KB (8 × 64 KB). Unit test covers the 512 KB path.
- Guided flash auto-correct accepts 128 KB / 512 KB / 2 MB.
- Live properties: Mode 09 VIN (0x02) + CALID (0x04) + Mode 01 PID 0x00 mask. Offline reports `UNREAD`.
- Fail-closed guided flash when not connected. Risk flag defaults to false.
- Live + logger feed for STFT, LTFT, MAF, VSS, engine load.
- Hex editor starts at the selected table address. Checksum summary is shown in the side panel.
- Scripts tab lists real bench helpers. `list_script_helpers` command added.
- UI sends `user_confirmed_risks` and auto-identifies a BIN on load.

## Still needs your bench / dumps

1. Exact EDC17 / MED17 seed tables from *your* dumps
2. Embedded Python runtime (PyO3) — intentionally not faked
3. Windows registry J2534 enumeration
4. Full tokio async I/O on multi-minute transfers
5. Hardware validation of 512 KB additive checksum vs PCM Hammer on *your* specific OS

## Safety

Never flash without a verified backup and stable power. Personal dumps only. This tool is free and honest about what it can and cannot prove without hardware.

Build your own. No bullshit prices.
