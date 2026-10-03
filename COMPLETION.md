# TuneItVerse v3.36.0 — image inspect

v3.35 supply watch, definition packs, and self-check stay. This pass adds the missing dump-review path that was still only a table-local hex tab.

## What this pass changed

1. Version **3.36.0** across crate / package / Tauri / installer / HTML.
2. Inspect page: hex+ASCII window, block entropy profile, markdown tune report. Commands: `bin_hex_window`, `bin_profile`, `bin_tune_report`.
3. Bench script commands `hex`, `profile`, `report`. Same rules: no eval, no shell, no write-flag flip.
4. CLI parity: `python3 python/ecu_scripting.py profile|hex|report`.
5. Self-check reports `inspect_ok` from a blank-buffer entropy probe.

Write path remains **P01_0411** and **EDC16C41** only. Profile and report do not identify a family and do not enable write.

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
