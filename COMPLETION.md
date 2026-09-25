# TuneItVerse v3.30.0 — honest ops surface

v3.29 claimed version chrome, `write_path_live` everywhere, and STFT+LTFT
blend. HTML was still 3.17 / overlay fallback 3.25. Catalog `write_allowed`
was a second hardcoded match. Map-from-log only averaged STFT.

## What this pass changed

1. Version **3.30.0** across package, crate, Cargo.lock, Tauri window,
   installer, HTML title/sidebar/status, overlay fallback, workspace
   export, and docs.
2. Catalog, identify `write_allowed`, and guided flash all call
   `ecu_database::write_path_live`. Runtime packs cannot turn write on.
3. Map-from-log now emits `stft_avg`, `ltft_avg`, `trim_blend` plus
   `trim_blend` counters. Preview math blends STFT+LTFT when both exist.
4. Runtime ECU pack import (`import_ecu_pack_cmd`) merges JSON for the
   session without a rebuild. Still identify/report unless the family is
   already on the live write list.
5. TunerPro-style XDF export from located tables (`export_tables_xdf`,
   Maps → Export XDF, bench `xdfexport`).
6. Honest capabilities matrix on the dashboard and via `capabilities`.
7. Catalog additions (identify only): MED9, EDC16C31/C34, GM E38.
8. Seed-family dropdown fills from the live catalog.

Write path remains **P01_0411** and **EDC16C41** only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / MED9 / SID803 / Honda / Simos / T8 / Transtron /
   E38 seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real
   FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a
   full backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
