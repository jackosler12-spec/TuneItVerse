# TuneItVerse v3.39.0 — wired desks and readiness

v3.38 desk tools stay. This pass fixes features that were written and never loaded, and adds the offline checklist the flash page was missing.

## What this pass changed

1. Version **3.39.0** across crate / package / Tauri / installer / HTML.
2. `index.html` now loads `v333.js` and `v334.js`. Seed algo / P01 compare and the script runner were present on disk and not included.
3. Flash → Readiness (`flash_readiness_cmd`) runs identify + checksum and always returns `flash_allowed_now: false`.
4. Flash → Seed coverage lists dump-derived pairs vs catalog families that are still empty.
5. Maps → CSV exports the open table. Clipboard copy when the browser allows it. Does not write the ECU.
6. Bench script commands `readiness` and `seedcov`. CLI `python3 python/ecu_scripting.py readiness dump.bin` is a size check only.
7. Self-check reports `desk_ops_ok`.

Write path remains **P01_0411** and **EDC16C41** only.

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
