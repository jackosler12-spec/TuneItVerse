# TuneItVerse v3.23.0 — GM 2-byte tables live, catalog honesty, seed lookup first

v3.22 shipped the catalog families and the maplog compile fix. What a tuner still could not do without another paid tool:

1. Compute a GM 2-byte key from a captured seed using the public algorithm-index tables (`reference/2byte-keys.txt`, 1280 rows, algo 0–1023).
2. Prefer a measured pair from `seed_tables.json` over starter algebra.
3. See which catalog families actually have a write path.

## What this pass changed

1. Version 3.23.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs. The sidebar no longer claims 3.17.0.
2. `gm_keys.rs` embeds and runs the Universal Patcher 2-byte opcode machine against `2byte-keys.txt`. Unit vectors: algo 0 seed `1234` → `3EF7`; algo 1 is a byte swap.
3. `compute_seed_key` order: measured `seed_tables.json` pair → optional GM algo index → P01/P59 LFSR → Bosch dispatcher (EDC16C41 measured, everything else fail-closed).
4. Connect page: algo field + EDC15 / ME9 / Simos18 / T8 / GM_2BYTE family entries.
5. Dashboard catalog `write_allowed` flag. Only P01_0411 and EDC16C41 advertise a live write path. Honda and P59 stay blocked.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`) using both the LFSR and the table algo your PCM actually uses.
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
6. Licensed GM 5-byte keys. We do not ship that library.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
