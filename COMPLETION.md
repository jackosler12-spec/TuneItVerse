# TuneItVerse v3.25.0 — P59 fail-closed checksum/identify, UDS progress abort

v3.24 documented P59 checksum blocking and identify flags that were not actually
wired in `checksum.rs` / `identify_bin`. HTML still said 3.17.0. UDS
`download_image` ignored progress-callback errors, so J2534 Vbatt sag could
not abort a Bosch write.

## What this pass changed

1. Version 3.25.0 across package, crate, Tauri window, installer, HTML, overlay,
   workspace export, and docs.
2. `checksum.rs` treats P59 the same as Honda: report-only validate, fail-closed
   correct. Unit test included.
3. Identify publishes `write_allowed`, `gm_p59_os`, and refuses P59 correction
   and write. `GM_P59` is never `correction_safe`.
4. `resolved_family` refuses P59 OS strings the same way it refuses Honda.
5. UDS `download_image` now propagates progress-callback `Err`, so mid-write
   J2534 Vbatt sag actually aborts.
6. P59 catalog OS list no longer includes Holden P01 `12225074` (that ID is P01).

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
