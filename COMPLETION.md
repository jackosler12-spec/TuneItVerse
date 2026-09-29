# TuneItVerse v3.33.0 — catalog/seed/adapter UI actually in HTML

v3.32 docs said the catalog Write column, Connect adapter list, and seed
`algo` argument were wired. The overlay painted seven cells into a six-column
table, `#adapter-list` did not exist, and Compute Key never sent `algo`.

## What this pass changed

1. Version **3.33.0** across crate / package / Tauri / installer / HTML / overlay.
2. Catalog `<thead>` includes **Write**. Overlay LIVE/blocked cells match.
3. Connect page has `#adapter-list` and loads `list_supported_adapters`.
4. Seed UI has optional GM 2-byte algo (0–1023) and a P01 vs table compare.
5. Identify chip / dashboard JSON show `write_allowed`.
6. Bench script `adapters` command lists the official adapter catalog.
7. Write path remains **P01_0411** and **EDC16C41** only.

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
