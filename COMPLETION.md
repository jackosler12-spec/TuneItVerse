# TuneItVerse v3.34.0 — Scripts page actually runs, catalog baked into HTML

v3.33 docs said the Scripts page ran `identify` / `checksum` / `adapters`
against the loaded BIN. The page only listed helper cards. HTML still said
3.17.0 and the Write column / adapter list / seed algo lived in a JS overlay.

## What this pass changed

1. Version **3.34.0** across crate / package / Tauri / installer / HTML / overlay.
2. Scripts page has a real runner: `run_bench_script` against the loaded BIN
   (and the last Compare image). `correct` / `poke` mutate the working image
   and enable Save BIN. No eval, no shell.
3. Bench language + helper list gained `adapters`. Python CLI gained
   `adapters` and `diff`.
4. Catalog `<thead>` includes **Write**. Connect page has `#adapter-list`.
   Seed UI includes GM algo + P01 vs table in HTML, not only overlay.
5. Seed family list matches the catalog (ME9 / EDC15 / Simos / T8 / Transtron).
6. Checksum validate names 1MB / 1.5MB / 4MB catalog sizes instead of UNKNOWN.
   Correction stays fail-closed for those sizes.
7. BIN compare adds a 64 KB region histogram and remembers the compare image
   for scripts.

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
