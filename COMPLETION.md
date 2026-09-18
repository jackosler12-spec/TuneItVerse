# TuneItVerse v3.22.0 — compile the maplog path and ship the catalog that v3.21 still lacked

v3.21 registered the script runtime but left holes that break a real build and a real tuner workflow:

1. `scripting.rs` calls `v29_tools::analyze_log`, and that function was still private. `cargo test --lib` cannot compile.
2. `map_from_log_cmd` ignored CSV. The Maps button only worked if a live session was already in the log buffer.
3. HTML / workspace export still said 3.17.0. Scripts view was a stub; the editor was injected only if JS ran.
4. No place to drop *your* measured seed/key pairs. EDC17/MED17 stayed starter-algebra-only.
5. Catalog stopped at nine families. 4 MB Simos dumps identified as unknown.

## What this pass actually changed

1. Version 3.22.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs.
2. `analyze_log` is `pub(crate)`. `map_from_log_cmd` accepts optional CSV and imports it before the occupancy heatmap.
3. `seed_tables.rs` embeds `reference/ecu_database/seed_tables.json`. Empty by default. A matching pair is treated as measured and can unlock; everything else stays fail-closed.
4. Catalog + loader: EDC15_COMMON, ME9_COMMON, SIMOS18 (4 MB), TRIONIC8. Writes stay blocked until you add a measured corrector.
5. Scripts page ships a textarea + Run + click-to-insert helpers. Compare stores the second image for `compare` scripts and prints diff ranges.
6. BIN compare now includes identify + SHA for both sides.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 seed tables from **your** dumps. Put pairs in `seed_tables.json`. Starter algebra is not a measured key.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
