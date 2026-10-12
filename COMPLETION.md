# TuneItVerse v3.40.0 — version sync + offline polish

v3.39 community Bosch multipoint CRC32 helper and desk tools stay. This pass synchronizes versions, expands documentation of the operational offline and limited live paths, and confirms fail-closed behavior.

## What this pass changed

1. Version **3.40.0** in package.json and documentation.
2. COMPLETION.md updated to accurately reflect the current state of the application.
3. Offline tools (identify, checksum, hex, profile, report, DTC explain, map delta, BIN undo) remain fully operational.
4. Live write path remains strictly limited to **P01_0411** and **EDC16C41**. All other families are identify/catalog only.
5. Guided flash, voltage gate, backup quality labeling, and live verify stay fail-closed.
6. Guided flash now refuses write unless backup quality is FullImage (PartialDidOnly and Failed both block). Backup is mandatory.

## Current operational status

- Full offline BIN/XDF/A2L inspection, patching, checksum validation/correction (P01 additive, EDC16 multipoint CRC32), map editing, and export.
- Catalog of 14+ ECU families with JSON definitions, TableSeek for P01, community map hints.
- Live diagnostics and logging via ELM/J2534 where hardware is present.
- Guided flash pipeline with safety gates for supported families only.

Write path remains **P01_0411** and **EDC16C41** only. Community correctors are never auto-applied on the live flash path.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. A kernel-resident Mode 3C full-image dump. Windowed probes stay PartialDidOnly and now block write until FullImage exists.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.

## Security access hardening (2026-10-12)
- `bosch_uds_unlock_full` is strict ISO 14229: seed must be `67 <level> <seed…>`, key ack must be exactly `67 <level+1>`. Empty, garbled, wrong SID/level = failure. NRC 0x35/0x36/0x37 give explicit errors; 0x78 on seed re-requests (max 3); 0x78 on key fails without resending the key. All-zero seed = already unlocked only inside a valid positive response.
- Starter/placeholder key algebra (EDC17, MED17, generic EDC16, generic Bosch) removed. These return `KeyError::KeyAlgorithmUnverified` and unlock refuses before any bus traffic. Only EDC16C41 (4-byte) and GM P01/P59 LFSR remain. Write stays limited to `ecu_database::write_path_live` (P01_0411, EDC16C41).
- Not bench-verified on hardware. The ELM/raw ISO-TP transport has no "wait for next frame" call, so key-step 0x78 is treated as locked.

## Honest-failure pass (2026-10-12)
- No P01/GM default for unknown ECUs: `live_verify::live_address_plan` errors on unknown/unrecognised families; verify/compare/backup refuse. Live windows outside the image are skipped (no offset-0 fallback). `compute_seed_key` and script `seedkey` require a family.
- Live logging: no connection → "No live connection" error; connected but no PID answered → error; nothing recorded. Simulation only via explicit Demo mode (`log_set_demo_mode`), samples flagged `demo:true`, CSV export headed `# DEMO DATA`, red DEMO badge in UI. Demo cannot be toggled mid-session and clears the buffer.
- J2534: unbound connect/write/read and ISO15765 filter return `NotImplemented` instead of fake Ok; `j2534_connect` no longer reports success when PassThruConnect fails.
- Versions synced to 3.40.0 (tauri.conf.json, Inno installer, UI console banner).

## J2534 filter, DB flash addresses, family plumbing, Demo toggle (2026-10-12)
- J2534: `PassThruStartMsgFilter` FLOW_CONTROL_FILTER for ISO15765 (mask 0x7FF / 0x1FFFFFFF, pattern = ECU rx id, FC = tester tx id, FC padded); real filter id returned and tracked; `PassThruStopMsgFilter` on disconnect/close. `j2534_connect` installs 0x7E8/0x7E0 or fails. Still `NotImplemented` without a loaded DLL. Not tested on a real device.
- Flash/verify address comes from `flash_write_address` in reference/ecu_database (P01_0411 0x20000, EDC16C41 0x80000, same as before). Families without an entry (ME7, EDC17, …) are refused before unlock.
- Frontend never defaults a family; unknown family shows an explicit error. Backend compare/verify refuse a family that contradicts the BIN.
- Logging screen gets a Demo mode checkbox (disabled while logging).
- KNOWN BLOCKER: src/index.html on main is a 14-line placeholder (no markup, no script tags) since the history-rewrite commit 21e4535. The UI does not load main.js at all until the real index.html is restored.

## UI restored (2026-10-12)
- src/index.html restored from Jack's v3.17.0 copy, version strings set to 3.40.0, and script tags added for the overlays that exist (v312, v333–v337). main.js and styles.css kept (current files are newer supersets of the supplied copies).
- Headless Chromium load test with a mocked Tauri invoke: 0 console/page errors; all 7 views render; logging Demo toggle + badge, seed/key and unlock controls present.
- `#dtc-lookup` (used by main.js explainDtcUi) has no element in the restored HTML; DTC explain lookup has no input yet.

## GM flash segment + UI fixes (2026-10-12)
- GM VPW write: level-2 unlock must succeed (was ignored); Mode 34/36/37 each need a positive reply. Writes only the P01_0411 Calibration segment 0x8000-0x1FFFF (source: reference/PcmHammer_debugLog_20260504@180810.txt line 7716). The old 0x20000 target is the OperatingSystem segment per the same log, and the full 512KB used to be sent there.
- BIN size must equal the DB image size (512KB for P01). EDC16C41 has no authoritative segment data in the repo, so its write and live verify are refused until one is added.
- Failed checksum correction now refuses the write instead of flashing the uncorrected image.
- Diagnostics: DTC lookup input `#dtc-lookup` + Explain button; output HTML-escaped. DEMO badge moved inline into the page header.

## P01 checksum + identify fixed against a real read (2026-10-12)
- The old P01 checksum regions (8 per 64KB block, CS at block+0x3FFE etc.) were invented: on a real 512KB OS 12225074 read all 64 "regions" failed, and correction would have written 64 words, many inside OS code. Replaced with the GM layout from reference/P01-P59.xml: OS wordsum (CS @0x500; 0-4FF, 502-3FFF, 20000-[@0x510]) plus 7 cal segments read from the table at 0x514, CS word at each segment start, 16-bit two's-complement wordsum. The table is read from the image and sanity-checked; otherwise correction refuses. Cal-only 128KB files are report-only (no table).
- Identify: P01 stores its OS ID as a binary u32 at 0x504, so the ASCII search missed it and the 512KB Honda size collision left family unresolved. The binary ID is now used when the OS checksum validates.
- APP_VERSION now comes from Cargo.toml.
- Optional real-image tests: set TIV_P01_BIN=/path/to/read.bin (skipped when unset). Never commit the image.
