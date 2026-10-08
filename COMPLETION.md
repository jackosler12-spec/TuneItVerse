# TuneItVerse v3.39.0 — ops desk

v3.38 desk tools stay. This pass adds the missing driver-plugin SDK, bench session planner, BIN fingerprint, and an honest coverage report. It does not add a write family.

## What this pass changed

1. Version **3.39.0** across crate / package / Tauri / installer / HTML.
2. Driver plugin SDK: built-in ELM327, OBDLink MX+, J2534, and FTDI descriptors. Pasted JSON is validated. `write` / `flash` / `unlock` capabilities are rejected. `write_allowed` is forced false.
3. Bench planner: family checklist for personal dumps. BDM/JTAG stays foundation-only. No programming frames.
4. Fingerprint: size, part string, and marker scoring against the catalog. Does not enable write.
5. Coverage desk: catalog checksum text vs the real Rust implementation (`implemented_additive16`, `implemented_crc32_multipoint`, blocked, or catalog-only).
6. Self-check reports `plugins_ok`, `bench_plan_ok`, `fingerprint_ok`, `coverage_ok`.

Write path remains **P01_0411** and **EDC16C41** only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.
8. Your own BDM/JTAG pinout notes. The planner will not invent a probe script.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
