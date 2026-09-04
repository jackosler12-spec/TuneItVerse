# TuneItVerse v3.7.1 — claimed v3.7.0 work actually shipped (2026-09-04)

**Status: v3.7.0 merged A2L + table math UI, but three items in that write-up were still lies. This pass implements them.**

## What this pass actually changed

1. `apply_math` / `inverse_math` now compact + uppercase the expression. Catalog `x*0.1` and TunerPro `X * 0.5` both work. Previously only exact `X*` matched, so extracts were raw counts.
2. `parse_xdf_definitions` reads TunerPro **XDFFORMAT** text (`XDFTABLE` / `EMBEDDEDDATA` / `MATH equation=`), then TableData / TableSeek as before.
3. Unknown BIN sizes get a **report-only** checksum report (`ecu_family=UNKNOWN`). `correct_checksums` still errors. No invented ME7/SID corrector.
4. Catalog starters: `SIEMENS_SID803` (1.5MB) and `HONDA_KEIHIN` (512KB, size-collides with P01 — identify by `37820-*` OS string).
5. Map-from-log stores `window.lastMapFromLog` so STFT preview can apply occupancy.
6. Connect UI exposes UDS as a protocol radio. Seed-key family list includes the new catalog IDs.
7. Versions synced to 3.7.1.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.
6. Honda vs P01 512KB collision: always confirm OS string before any P01 additive correction.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
