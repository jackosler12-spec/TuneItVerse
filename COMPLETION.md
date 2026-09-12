# TuneItVerse v3.18.0 — live flash progress + P59 fail-closed + CLI diff

v3.17 documented `flash-progress` events and a bench `diff` command. The tree on main did not match that write-up:

1. `guided_flash_pipeline` still called `orchestrate_guided_flash(..., |_| {})`, so the flash bar only jumped after the invoke returned.
2. Identify marked `GM_P59` as `correction_safe`. Checksum correct and guided write only blocked Honda, not P59 OS strings (`12586243` / `12602801`).
3. `python/ecu_scripting.py` had no `diff` subcommand even though README / COMPLETION listed it.

## What this pass actually changed

1. Version 3.18.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs.
2. `guided_flash_pipeline` takes `AppHandle` and emits `flash-progress` `{percent, bytes_done, bytes_total}` on every VPW chunk and UDS download tick. UI listens and updates the bar live.
3. Identify publishes `gm_p59_os`. `correction_safe` is only true for measured P01 (not P59) and EDC16 family. `resolved_family` refuses P59 write/compare.
4. `checksum::validate` is report-only for P59. `correct_checksums` and guided flash refuse P59 OS strings and the `GM_P59` family write path.
5. CLI `python3 python/ecu_scripting.py diff stock.bin tuned.bin` matches `compare_bins_cmd` ranges. Scripts view lists the command.

## Still needs your bench

1. Measured P59 checksum words and kernel. Do not invent them.
2. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.

---

# TuneItVerse v3.17.0 — table descriptions + orbitable 3D maps

Parameter titles use Universal Patcher ExtraTableName when present, otherwise a readable form of the pack name (`Volumetric_Efficiency_Crank` → `Volumetric Efficiency Crank`). Each map shows its TableSeek description under the editor and in **What this table does**. The 3D tab is an orbitable surface (drag rotate, wheel zoom), not a flat heatmap. 1×N tables draw as a 2D line. Descriptions come from the pack — no invented tune advice.
