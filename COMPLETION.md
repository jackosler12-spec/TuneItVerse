# TuneItVerse v3.30.0 — UDS voltage abort + honest chrome

v3.29 documented mid-flash voltage and a catalog Write column. The HTML
thead was still six columns, static chrome was still 3.17, workspace
export said 3.27, and `uds::download_image` took `FnMut(u32, u32)` so
the Result-returning sag-abort closure in `flash.rs` could not compile
or abort a UDS write.

## What this pass changed

1. Version **3.30.0** from one crate constant (`APP_VERSION`) plus
   package / Tauri / installer / HTML / overlay fallbacks.
2. `download_image` now reports progress as
   `FnMut(&mut Port, u32, u32) -> Result<(), String>` and probes PID
   0x42 every four blocks. J2534 Vbatt sag still aborts in the flash
   callback. VPW still re-checks every five chunks.
3. Catalog table header includes **Write**. `list_ecu_catalog` and
   identify `write_allowed` both go through `write_path_live`.
4. Connect page lists official adapters from
   `list_supported_adapters`. Seed family dropdown fills from the
   catalog and accepts an optional GM 2-byte algo index (0–1023).
5. Workspace export version matches the crate.

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
