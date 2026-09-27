# TuneItVerse v3.31.0 — operational honesty pass

v3.30 documented catalog Write, crate-version workspace export, seed
family-from-catalog, and a GM algo field. The HTML thead was still six
columns, `export_workspace_cmd` still hardcoded `3.27.0`, the Connect
page never rendered `list_supported_adapters`, and Compute Key never
sent `algo`.

## What this pass changed

1. Version **3.31.0** from `APP_VERSION` plus package / Tauri / installer
   / HTML fallbacks. Overlay reads `app_info` and paints the sidebar and
   status bar from the crate.
2. Workspace export uses `crate::APP_VERSION` and publishes
   `write_families`. Identify reports `app_version` + `write_families`.
3. Catalog table header includes **Write**. Seed family dropdown is
   filled from `list_ecu_catalog`. Optional GM 2-byte algo index
   (0–1023) is sent to `compute_seed_key`.
4. Connect page lists official adapters from `list_supported_adapters`.

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
