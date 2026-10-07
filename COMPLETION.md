# TuneItVerse v3.38.0 — desk tools

v3.37 log studio stays. This pass closes three offline desk gaps. It does not add a write family.

## What this pass changed

1. Version **3.38.0** across crate / package / Tauri / installer / HTML.
2. Diagnostics: offline `explain_dtcs_cmd` for SAE codes, including rail/boost/EGR codes used on the Patrol. Lookup does not talk to an ECU.
3. Maps: reference BIN + `table_delta_cmd` cell report. Clamp and percent on the selected cells. Table redo. BIN undo/redo snapshots before poke and table patch (8 images).
4. Bench CLI: `dtc P0087`. Scripts list includes dtc and delta. Self-check reports `dtc_explain_ok`.
5. Clamp/percent also exist on the Rust table math path. Neither path enables write.

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
