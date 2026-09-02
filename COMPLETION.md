# TuneItVerse v3.5.0 — catalog wire + identify hashes + UI overlay (2026-09-02)

**Status: v3.4.0 shipped live Mode 23/3C verify. This pass wires the catalog and UI that v3.4.0 documented but did not actually load.**

v3.4.0 added `me7_common.json` / `delphi_dcm.json` and `src/v340.js`. On main the Rust loader never `include_str!`'d those JSON files, `index.html` still said v3.3.0 and never included `v340.js`, and `identify_bin` did not compute SHA-256 or printable strings.

## What this pass actually changed

1. `ecu_database.rs` embeds `ME7_COMMON` and `DELPHI_DCM`. Table auto-load merges refined_map_addrs from every family that shares a BIN size, including gasoline keys (ignition / VE / boost).
2. `identify_bin` reports SHA-256 (full + first 4K + last 4K) and printable ASCII strings ≥ 6 chars.
3. `compare_bins` reports contiguous diff ranges, not only the first 40 byte hits.
4. Map-from-log occupancy grid also averages STFT when the log has that channel.
5. `compare_bin_to_ecu` runs the same live windows as verify instead of a CRC-only stub.
6. `index.html` is v3.5.0, loads `v340.js`, has Export workspace + accept-unverified-write, and seed-family includes ME7 / Delphi.
7. `var currentBin` + `window.invokeCmd` so overlays can see the loaded image.
8. Offline DTC clear is fail-closed (`success: false`).
9. 1MB dumps are recognized as ME7-sized. Checksum correction is **not** invented — validate reports the family, correct refuses.
10. `j2534_list_devices` runs `reg query` on `PassThruSupport.04.04` (+ Wow6432Node) on Windows.

## Still needs your bench

1. EDC17 / MED17 / ME7 seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on a Windows box to confirm the registry walk finds the real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a 512 KB / 1 MB / 2 MB backup.
5. ME7 block checksum routine measured on a personal dump before we ship a corrector.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
