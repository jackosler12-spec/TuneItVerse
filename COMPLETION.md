# TuneItVerse v3.35.0 — supply watch, definition packs, self-check

v3.34 scripts runner stays. This pass closes three software gaps that were still open on main.

## What this pass changed

1. Version **3.35.0** across crate / package / Tauri / installer / HTML.
2. Mid-write supply monitor uses **J2534 Vbatt** only. Serial Mode 01 PID 0x42 is no longer sent during a Mode 36 / UDS transfer (that request can abort programming). Sag aborts the transfer and sets `voltage_aborted`. Samples land on the flash result.
3. Community definition packs (`import_definition_pack`). A pack cannot flip `write_allowed`. Example: `reference/ecu_database/example_definition_pack.json`.
4. Offline `operational_self_check` plus Scripts → Self-check and Maps → Import definition pack.
5. HTML chrome is 3.35.0 without waiting for the overlay.

Write path remains **P01_0411** and **EDC16C41** only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
