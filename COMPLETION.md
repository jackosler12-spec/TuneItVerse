# TuneItVerse v3.7.0 — XDF/A2L + table math + honest checksum report (2026-09-04)

**Status: v3.6.0 identified by OS string and listed J2534 devices. This pass closes three software gaps that broke daily tuning work.**

## What this pass actually changed

1. Table conversion math is case-insensitive. Catalog defs use `x*0.1`; extract/patch used to treat that as a no-op.
2. `parse_xdf_definitions` reads TunerPro **XDFFORMAT** text XML (`XDFTABLE` / `XDFAXIS` / `EMBEDDEDDATA`), not only TableData / TableSeek.
3. A2L scanner (`parse_a2l_definitions`) pulls CHARACTERISTIC name + address hints from a personal ASAP2 file.
4. Table tools: scale, offset, 3x3 smooth, STFT occupancy preview. Preview does **not** write flash. Patch + checksum still required.
5. Unknown BIN sizes get a **report-only** checksum. Correction stays fail-closed. No invented ME7 corrector.
6. Live Mode 01 dashboard now includes O2 B1S2 and fuel level (already in the logger).
7. Versions synced to 3.7.0.

## Still needs your bench

1. EDC17 / MED17 / ME7 seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 block checksum routine measured on a personal dump before a corrector ships.
6. Axis labels from a real XDF/A2L — the parsers store addresses, not a verified axis map for every OS.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
