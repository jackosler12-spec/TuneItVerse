# TuneItVerse v3.29.0 — fail-closed write + honest chrome

v3.28 claimed version chrome was synced and map-from-log blended LTFT.
HTML was still 3.17 / overlay fallback 3.25. Guided VPW voltage still
rechecked every 10 chunks. `orchestrate_guided_flash` would attempt a
write for any family whose name contained EDC/MED/BOSCH/DELPHI/SID/P01/GM.

## What this pass changed

1. Version **3.29.0** across package, crate, Cargo.lock, Tauri window,
   installer, HTML title/sidebar/status, overlay fallback, workspace
   export, and docs.
2. Single source of truth: `ecu_database::write_path_live`. Catalog,
   identify `write_allowed`, and guided flash all use it.
3. Guided flash refuses every family except **P01_0411** and **EDC16C41**.
4. VPW mid-write voltage gate now re-checks PID 0x42 every 5 chunks.
5. Map-from-log trim grid blends STFT + LTFT when both channels exist
   (`trim_blend` counters in the report). Preview still never writes.
6. Connect page lists official adapters from
   `reference/adapters/supported_adapters.json`.
7. Dashboard catalog table has a Write column that matches the live path.
8. Seed-key family dropdown includes the full catalog.

Write path remains **P01_0411** and **EDC16C41** only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed
   tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real
   FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a
   full backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
