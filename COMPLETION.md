# TuneItVerse v3.39.0 — community checksum + desk polish

v3.38 desk tools stay. This pass adds an explicit community multipoint CRC32 helper for 2 MB Bosch-style images and bumps the version.

## What this pass changed

1. Version **3.39.0** across crate / package / Tauri / installer / HTML.
2. New `community_bosch_correct_cmd` exposes a multipoint CRC32 corrector using EDC16-style regions for 2 MB images. Labeled community / verify-before-flash. Live write path remains **P01_0411** and **EDC16C41** only.
3. Identification, map catalog, seed-table lookup, voltage gate, and guided flash fail-closed behavior unchanged.
4. Offline DTC explain, table delta, clamp/percent, BIN undo/redo remain.

Write path remains **P01_0411** and **EDC16C41** only. Community corrector is never auto-applied on the live flash path.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
