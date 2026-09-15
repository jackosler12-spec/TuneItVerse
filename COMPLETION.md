# TuneItVerse v3.20.0 — wire the runtime that v3.19 documented

v3.19 added `scripting.rs` and claimed `guided_flash_pipeline` emitted `flash-progress`. On main those were not wired:

1. `mod scripting` was missing. `run_bench_script` was never in the invoke handler. `list_script_helpers` in `lib.rs` still returned Python CLI strings. The Scripts page had no editor.
2. `guided_flash_pipeline` still called `orchestrate_guided_flash(..., |_| {})`, so the bar only jumped when the invoke returned.
3. `v29_tools::analyze_log` was private, so the script runtime could not compile even if it had been registered.
4. UDS download progress could not abort on J2534 Vbatt sag.

## What this pass actually changed

1. Version 3.20.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs.
2. `mod scripting` + `run_bench_script` / `list_script_helpers` registered. Scripts view is an editor + Run + click-to-insert helpers. Mutating commands (`poke`, `correct`) return the working BIN so Save stays honest.
3. `guided_flash_pipeline` takes `AppHandle` and emits `flash-progress` `{percent, bytes_done, bytes_total, voltage_warn}` on every VPW chunk and UDS tick. The Flash UI listens and moves the bar live.
4. UDS `download_image` progress callback can fail. J2534 Vbatt below the gate aborts mid-write; near-gate values surface as `voltage_warn`. VPW path still abort-gates every 10 chunks.
5. `map_from_log_cmd` accepts optional CSV and imports it into the log buffer before the occupancy heatmap.
6. `with_port` is `pub(crate)` so `v312` voltage helpers compile.

## Still needs your bench

1. Measured P59 checksum words and kernel. Do not invent them.
2. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
