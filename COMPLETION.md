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
