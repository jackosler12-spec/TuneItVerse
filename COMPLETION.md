# TuneItVerse v3.28.0 — software-first ops pass

v3.27 published identify `write_allowed` / `gm_p59_os`. HTML chrome was still
drifting (3.17 / 3.25). Product direction still implied in-house VerseLink PCB
work. Map-from-log occupancy ignored LTFT.

## What this pass changed

1. Version 3.28.0 across package, crate, Cargo, Tauri window, installer,
   HTML title/sidebar/status, overlay fallback, workspace export, and docs.
2. Software-first product record: `PRODUCT.md` plus
   `reference/adapters/supported_adapters.json`. Connect page lists official
   adapters. VerseLink Apex PCB/KiCad is parked.
3. Catalog family `TRANSTRON_4HK1` (Isuzu FRR class) — identify / map hints
   only. No write path.
4. Map-from-log trim grid blends STFT + LTFT when both channels are logged.
   `apply_stft_preview` still never writes the ECU.
5. Guided VPW flash re-checks PID 0x42 every 5 chunks (was 10) and aborts on
   sag. J2534 Vbatt abort was already live.
6. Scripts helper `adapters` dumps the official adapter JSON.

Write path remains **P01_0411** and **EDC16C41** only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables
   from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`) using both the LFSR
   and the table algo your PCM actually uses.
4. A vendor J2534 DLL on Windows so the registry walk returns a real
   FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full
   backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
