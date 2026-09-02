# TuneItVerse v3.5.1 — live ISO-TP verify + log PIDs (2026-09-02)

**Status: v3.5.0 wired the catalog and UI overlay. This pass fixes the live UDS path and the commands that still lied.**

## What this pass actually changed

1. `live_verify` no longer sends a raw ALFI payload over VPW `request_response`. Bosch/ME7/Delphi windows use `uds::read_memory_by_address` (ISO-TP SID 0x23).
2. `compare_bin_to_ecu` probes those same windows instead of printing a CRC stub.
3. `verify_after_write` uses the connected OS → family, not a hard-coded P01 string.
4. `log_capture_sample` pulls STFT / LTFT / MAF / VSS / load / IAT / spark so map-from-log heatmaps can average STFT.
5. Offline DTC clear is fail-closed (`success: false`).
6. `currentBin` is a `var` so workspace export can see the loaded image.

## Still needs your bench

1. EDC17 / MED17 / ME7 seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows to confirm the registry walk.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 block checksum routine measured on a personal dump before a corrector ships.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
