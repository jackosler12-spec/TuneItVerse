# TuneItVerse v3.41.0 — P01 backup live; write still refused; EDC16 honest

## Flash/backup audit log (2026-10-14)
- Every guided flash call and every P01 full backup is appended to `<app data>/audit.jsonl` (JSON Lines, opened append-only, never rewritten).
- Each line: timestamp, kind, family, OS id, image/backup SHA-256, gates passed, the gate that refused, result (`ok`/`refused`/`failed`), detail. No VIN, serials or image bytes.
- Flash screen → "Flash & backup audit log" panel shows the last 200 entries (read-only).
- Append-only is by convention: the file is a normal user file and can be edited or deleted outside the app.

## v3.41.0 changes (2026-10-13)

1. **P01 kernel write:** not implemented. `reference/PcmHammer_debugLog_20260504@180810.txt` is a read-only session (512× mode-35 reads, then halt). No erase/write frames exist in the repo, so after FullImage backup + unlock + checksum + OS match + voltage gates pass, guided flash returns an explicit refusal rather than inventing protocol bytes.
2. **EDC16C41:** checksum correction disabled (report-only). `EDC16_CRC32_REGIONS` fails on `NissanPatrol_2010_EDC16C41_392203.bin`, `EDC16C41 working flash`, and the WinOLS-corrected 392203 image. Write path removed from `write_families()`. Seed/key for EDC16C41 remains.
3. **Offline polish:** binary/encrypted XDF → clear error; grid uses extract `axes_x`/`axes_y` when definition headers are empty; Redo button + Ctrl+Y; duplicate v333/v334 script tags removed (v312 already loads them).
4. **ECU DB:** statuses rewritten to measured facts; P01 entry documents flash segment + verified flags; EDC16 entry documents what is missing.
5. Version **3.41.0** everywhere (Cargo.toml, Cargo.lock, tauri.conf, package.json, installer, UI).

## What is / is not operational

| Area | Status |
|------|--------|
| P01 FullImage backup protocol | Implemented (fake-PCM tested). STN link + real PCM untested. |
| P01 guided write | Refused after gates — no logged kernel erase/write. |
| EDC16C41 checksum | Report-only. |
| EDC16C41 write | Off. |
| Offline map editor / BIN compare / log replay | Present; UI load-tested with mocked invoke. |
| Hardware (ELM / J2534 / STN) | Not bench-tested on this tree. |

## Current operational status

- Offline BIN / text-XDF / A2L inspection and patching; P01 CS1+CVN validation/correction; EDC16 report-only.
- Catalog of 14 ECU families (most identify/report-only). TableSeek for P01.
- Live diagnostics/logging code paths exist; not bench-tested here.
- Guided flash gates stay fail-closed. After a verified FullImage backup, P01 write is still refused (no logged kernel write). No other family has a write path.

Never flash without a verified backup and stable power. Personal dumps only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. A logged P01 kernel erase/write session (the current log is read-only). FullImage backup already exists.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.

## Security access hardening (2026-10-12)
- `bosch_uds_unlock_full` is strict ISO 14229: seed must be `67 <level> <seed…>`, key ack must be exactly `67 <level+1>`. Empty, garbled, wrong SID/level = failure. NRC 0x35/0x36/0x37 give explicit errors; 0x78 on seed re-requests (max 3); 0x78 on key fails without resending the key. All-zero seed = already unlocked only inside a valid positive response.
- Starter/placeholder key algebra (EDC17, MED17, generic EDC16, generic Bosch) removed. These return `KeyError::KeyAlgorithmUnverified` and unlock refuses before any bus traffic. Only EDC16C41 (4-byte) and GM P01/P59 LFSR remain. Write stays limited to `ecu_database::write_path_live` (P01_0411 only); that path then refuses because the PcmHammer log has no kernel write session.
- Not bench-verified on hardware. The ELM/raw ISO-TP transport has no "wait for next frame" call, so key-step 0x78 is treated as locked.

## Honest-failure pass (2026-10-12)
- No P01/GM default for unknown ECUs: `live_verify::live_address_plan` errors on unknown/unrecognised families; verify/compare/backup refuse. Live windows outside the image are skipped (no offset-0 fallback). `compute_seed_key` and script `seedkey` require a family.
- Live logging: no connection → "No live connection" error; connected but no PID answered → error; nothing recorded. Simulation only via explicit Demo mode (`log_set_demo_mode`), samples flagged `demo:true`, CSV export headed `# DEMO DATA`, red DEMO badge in UI. Demo cannot be toggled mid-session and clears the buffer.
- J2534: unbound connect/write/read and ISO15765 filter return `NotImplemented` instead of fake Ok; `j2534_connect` no longer reports success when PassThruConnect fails.
- Versions synced to 3.41.0 (tauri.conf.json, Inno installer, UI console banner).

## J2534 filter, DB flash addresses, family plumbing, Demo toggle (2026-10-12)
- J2534: `PassThruStartMsgFilter` FLOW_CONTROL_FILTER for ISO15765 (mask 0x7FF / 0x1FFFFFFF, pattern = ECU rx id, FC = tester tx id, FC padded); real filter id returned and tracked; `PassThruStopMsgFilter` on disconnect/close. `j2534_connect` installs 0x7E8/0x7E0 or fails. Still `NotImplemented` without a loaded DLL. Not tested on a real device.
- Flash/verify address comes from `flash_write_address` in reference/ecu_database (P01_0411 0x20000, EDC16C41 0x80000, same as before). Families without an entry (ME7, EDC17, …) are refused before unlock.
- Frontend never defaults a family; unknown family shows an explicit error. Backend compare/verify refuse a family that contradicts the BIN.
- Logging screen gets a Demo mode checkbox (disabled while logging).
- KNOWN BLOCKER: src/index.html on main is a 14-line placeholder (no markup, no script tags) since the history-rewrite commit 21e4535. The UI does not load main.js at all until the real index.html is restored.

## UI restored (2026-10-12)
- src/index.html restored from Jack's v3.17.0 copy, version strings set to 3.41.0, and script tags added for the overlays that exist (v312, v333–v337). main.js and styles.css kept (current files are newer supersets of the supplied copies).
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

## P01 CVN (CS2) + table offsets (2026-10-12)
- CS2 per reference/P01-P59.xml: Checksum2Method=crc16, CS2Address empty (the value is not stored in the image; it is the CVN, CVN=2), CS2SwapBytes=true, CS2Blocks OS 0-3FFF+20000-@510 and each cal segment whole (covers the CS1 word). The XML doesn't name the CRC variant; CRC-16/ARC (0xA001 reflected, init 0) reproduces the stock CVN 717A for OS 12225074 BF in reference/stockcvn.xml, so that is what is used. Correction fixes CS1 first, then recomputes and reports CVNs. Reports compare against stockcvn.xml where the PN/segment is listed.
- Table addresses: removed the +0x20000 "cal base" applied to non-TableSeek tables on images >=160KB (XDF tables on P01, ME7/EDC catalog tables were all read 0x20000 too high). All addresses are file offsets. Honda catalog addresses have no authoritative base in the repo and are refused (extract notes it, patch refuses).

## P01 full read-back backup (OS 12225074)

- `src-tauri/src/p01_backup.rs`: OS query 3C0A, chatter off 2800, seed/key 2701/2702 (algo 0x28),
  kernel upload 34 + eight 36 blocks of `reference/Kernel-P01.bin` (last block sub 0x80 = execute),
  kernel version 3D00, flash ID 3D01 (only Intel 28F400B accepted), 512 reads of 35 01 0400 <addr>,
  block-sum checked, 3 retries, halt 20. Every frame cites `reference/PcmHammer_debugLog_20260504@180810.txt`.
- FullImage only if all 524288 bytes read, OS ID @0x504 = PCM OS, segment table parses, all 8 CS1 pass.
- Saved as `<app data>/backups/p01_<os>_<ms>.bin` + `.json` (SHA-256, quality). Guided flash for
  P01_0411 needs `backup_path`; it re-hashes, re-assesses, and refuses a tuned image with a different OS.
- UI: "P01 full backup" button on the flash view (v340.js) shows progress, path, hash, quality.
- Unverified: STN STPX link (`StnLink`) is not bench-tested; only OS 12225074 / 28F400B are accepted.
  Tests use a scripted fake PCM; `TIV_P01_BIN` runs the same against a private real image.

## v3.41.0 honesty notes
- Headless Chromium load test (mocked Tauri invoke): 0 console/page errors; all 7 screens open; screenshots under /workspace/ui_audit_*.png on the agent box.
- Nothing in this pass was tested on a real PCM, STN adapter, ELM, or J2534 device.
