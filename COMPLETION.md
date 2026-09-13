# TuneItVerse v3.20.0 — actually wire scripts + flash-progress

v3.19.0 added `src-tauri/src/scripting.rs` and wrote that `guided_flash_pipeline` emitted `flash-progress`. On main that was not true:

1. `lib.rs` never had `mod scripting`, so the runtime and `run_bench_script` were dead code.
2. `guided_flash_pipeline` still called `orchestrate_guided_flash(..., |_| {})`. The Flash bar only jumped when invoke returned.
3. Scripts UI still listed Python CLI strings and said it was not an interpreter.
4. HTML / status bar still said 3.17.0.

## What this pass actually changed

1. Version 3.20.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs.
2. `mod scripting` + `run_bench_script` / `list_script_helpers` registered. Scripts page is an editor that runs `identify`, `checksum`, `correct`, `compare`, `seedkey`, `poke`, `tables`, `maplog` against the loaded BIN. Mutating commands return bytes so Save stays honest.
3. `guided_flash_pipeline` takes `AppHandle` and emits `flash-progress` `{percent, bytes_done, bytes_total, voltage_v, voltage_warn}` on every VPW chunk and UDS download tick. UI listens. J2534 Vbatt sag mid-write aborts when the gate is on.
4. `map_from_log_cmd` accepts optional CSV and imports it into the log buffer before the occupancy heatmap.

## Still needs your bench

1. Measured P59 checksum words and kernel. Do not invent them.
2. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
