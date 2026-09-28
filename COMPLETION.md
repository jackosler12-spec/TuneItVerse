# TuneItVerse v3.32.0 — UI honesty + UDS mid-write abort

v3.31 claimed a catalog Write column, adapter list, and seed `algo` argument.
The overlay wrote seven catalog cells into a six-column table, `loadAdapters`
looked for `#adapter-list` that did not exist, and Compute Key never sent
`algo`. `uds::download_image` also rejected the fallible progress callback
that `flash.rs` already passed — mid-write J2534 Vbatt abort could not build.

## What this pass changed

1. Version **3.32.0** across crate / package / Tauri / installer / HTML.
2. Catalog `<thead>` includes **Write**. Overlay still paints LIVE/blocked.
3. Connect page renders official adapters from `list_supported_adapters`.
4. Seed UI has optional GM 2-byte algo (0–1023) and a P01 vs table compare.
5. Identify chip / dashboard JSON show `write_allowed`.
6. `download_image` progress is `FnMut(u32, u32) -> Result<(), String>` so
   J2534 Vbatt sag aborts a Bosch UDS transfer. Serial PID 0x42 stays
   pre/post — the port is owned by ISO-TP during the write.
7. Map-from-log adds LTFT grid + occupancy CSV. Still preview only.

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
