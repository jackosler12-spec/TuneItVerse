# TuneItVerse v3.2.0 — Honest operational pass (2026-08-30)

**Status: v3.2.0 closes gaps that v3.1.0 documentation still overstated.**

v3.1.0 COMPLETION.md said 512 KB P01 checksums, risk-flag default false, hex-at-table-address, identify-on-load, and scripts-tab helpers were landed. On `main` they were not:

- `checksum.rs` still rejected anything except 131072 bytes. Real LS1 dumps are 524288. `checksum_sizes.rs` already knew about 512 KB; the live path did not use it.
- `GuidedFlashRequest.user_confirmed_risks` still defaulted to **true** via `default_true`. Omitting the field would flash.
- Guided flash auto-correct only ran on 128 KB images.
- Hex dump in the UI still started at `0x20000`.
- BIN load did not call `identify_bin_cmd`.
- Scripts tab still listed logging templates instead of `list_script_helpers`.
- Flash UI request omitted `user_confirmed_risks`.
- Browser mock flash returned `success: true`.
- J2534 device list was a hardcoded marketing string, not a registry walk.
- Bosch unlock offline path returned `success: true`.
- 512 KB / 2 MB size collisions (P01 vs P59, EDC16 vs EDC17 vs MED17) were hidden — identify reported one family only.

## What this pass implements

- P01 additive checksums on 128 KB (2 × 64 KB) **and** 512 KB (8 × 64 KB). Unit test covers the 512 KB path.
- Guided flash auto-correct accepts 128 KB / 512 KB / 2 MB.
- `user_confirmed_risks` defaults to **false**. Pipeline aborts at the start if the UI did not confirm.
- UI sends `user_confirmed_risks` only when all four risk boxes are checked.
- Hex editor starts at the selected table address (0 if none).
- Checksum summary is written into the side panel.
- BIN load auto-identifies. Identify reports **all** families that share the image size.
- Scripts tab lists real bench helpers from `list_script_helpers`.
- CSV import (`log_import_csv`) so map-from-log works on a previous session.
- Offline `compute_seed_key` for P01 LFSR and EDC16C41 4-byte (does not unlock a bus).
- Connect tab: J2534 connect path, L1/L2/Bosch unlock buttons.
- J2534 list walks `HKLM\SOFTWARE\PassThruSupport.04.04` on Windows (`reg query`). Linux stays honest.
- Offline Bosch unlock and mock flash are fail-closed.

## Still needs your bench / dumps

1. Exact EDC17 / MED17 seed tables from *your* dumps (starters only — not faked as complete)
2. Embedded Python runtime (PyO3) — intentionally not faked
3. Hardware validation of 512 KB additive checksum vs PCM Hammer on *your* specific OS
4. Full tokio async I/O on multi-minute transfers
5. Live J2534 on a Windows box with a vendor DLL actually installed

## Safety

Never flash without a verified backup and stable power. Personal dumps only. This tool is free and honest about what it can and cannot prove without hardware.

Build your own. No bullshit prices.
