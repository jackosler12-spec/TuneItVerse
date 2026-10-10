# TuneItVerse v3.39.0 — expanded catalog heuristics

v3.38 desk tools stay. This pass expands auto-map starting points for more Bosch families and syncs the version. It does not add a write family.

## What this pass changed

1. Version **3.39.0** across crate / package / Tauri / installer / HTML / lib.
2. ecu_database.rs: more map key heuristics in tables_from_addrs (added common Bosch keys: lambda, boost_limiter, rail_limiter, etc.) so size-catalog packs surface more useful starting tables for EDC17/MED17/ME7-sized dumps.
3. Docs and index updated. Fail-closed write path unchanged.

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
