# TuneItVerse v3.21.0 — actually wire the runtime that v3.20 documented

v3.20 docs claimed `scripting.rs` and live `flash-progress` were on main. They were not:

1. `mod scripting` was missing. `run_bench_script` was never in the invoke handler. `list_script_helpers` in `lib.rs` still returned Python CLI strings. The Scripts page had no editor.
2. `guided_flash_pipeline` still called `orchestrate_guided_flash(..., |_| {})`, so the bar only jumped when the invoke returned.
3. `v29_tools::analyze_log` was private, so the script runtime could not compile even if it had been registered.
4. `with_port` and `compute_seed_key` were crate-root private, so `v312` / `scripting` could not call them.
5. Version strings were split across 3.17.0 (HTML) and 3.20.0 (crate / installer).

## What this pass actually changed

1. Version 3.21.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs.
2. `mod scripting` + `run_bench_script` / `list_script_helpers` registered. Scripts view is an editor + Run + click-to-insert helpers. Mutating commands (`poke`, `correct`) return the working BIN so Save stays honest.
3. `guided_flash_pipeline` takes `AppHandle` and emits `flash-progress` `{percent, bytes_done, bytes_total, voltage_warn}` on every VPW chunk and UDS tick. The Flash UI listens and moves the bar live.
4. `analyze_log` is `pub(crate)`. `map_from_log_cmd` accepts optional CSV and imports it into the log buffer before the occupancy heatmap.
5. `with_port` and `compute_seed_key` are `pub(crate)` so voltage helpers and `seedkey` scripts compile.

## Still needs your bench

1. Measured P59 checksum words and kernel. Do not invent them. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
