# TuneItVerse v3.27.0 — identify write flags actually published

v3.26 docs claimed `identify_bin` published `write_allowed` / `gm_p59_os` and
that P59 was never `correction_safe`. The HTML chrome was still 3.17.0 and
`identify_bin` still treated P59 like a safe P01 additive target.

## What this pass changed

1. Version 3.27.0 across package, crate, Cargo.lock, Tauri window, installer,
   HTML title/sidebar/status, overlay fallback, workspace export, and docs.
2. `identify_bin` now publishes `gm_p59_os` and `write_allowed`.
   P59 OS strings resolve to `GM_P59`. `correction_safe` is only true for P01
   (no Honda/P59 markers) and size-matched EDC16C41.
3. `resolved_family` refuses Honda and P59 the same way flash already did.
4. Catalog table header has a Write column (LIVE vs blocked).
5. Offline seed UI accepts an optional GM 2-byte algo index (0-1023).
6. Log channel list includes O2 B1S2, baro, fuel level, and fuel-system status
   Mode 01 PIDs the decoder already knew.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 seed tables from **your**
   dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`) using both the LFSR
   and the table algo your PCM actually uses.
4. A vendor J2534 DLL on Windows so the registry walk returns a real
   FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full
   backup.
6. Licensed GM 5-byte keys. We do not ship that library.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
