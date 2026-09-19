# TuneItVerse v3.24.0 — P59 checksum guard, catalog write column, seed algo field

v3.23 shipped GM 2-byte tables. What was still broken or dishonest in the running app:

1. HTML / overlay still said 3.17.0 / 3.21.0 while the crate was 3.23.0.
2. Catalog JSON already had `write_allowed`, but the dashboard table never showed it.
3. Connect page was missing the GM algo field and several catalog families.
4. P59 OS on a 512 KB image could still hit the P01 additive corrector in `checksum.rs`.
5. Identify marked P59 as `correction_safe`.

## What this pass changed

1. Version 3.24.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs. UI now prefers `app_info.version` at boot.
2. `checksum.rs` treats P59 the same as Honda: report-only validate, fail-closed correct. Unit test included.
3. Identify publishes `write_allowed`, `gm_p59_os`, and refuses P59 correction.
4. Dashboard catalog has a Write column (`LIVE` vs `blocked`). Only P01_0411 and EDC16C41 stay live.
5. Seed UI: algo 0–1023 plus EDC15 / ME9 / Simos18 / T8 / GM_2BYTE. `compute_seed_key` requires algo for GM_2BYTE.
6. Map-from-log occupancy rings the hottest cell on the current grid when a log exists. Hint only — not auto-write.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`) using both the LFSR and the table algo your PCM actually uses.
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
6. Licensed GM 5-byte keys. We do not ship that library.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
