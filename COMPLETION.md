# TuneItVerse v3.38.0 — flash preflight gate

v3.37 log studio stays. This pass closes the hole where Guided flash could be armed without an identify/checksum gate.

## What this pass changed

1. Version **3.38.0** across crate / package / Tauri / installer / HTML. Window title was still 3.34.0; it is now 3.38.0.
2. `flash_preflight_cmd` identifies the loaded BIN, runs the measured checksum validator, and returns blockers. `ready_for_guided_flash` is true only for a catalog-sized P01_0411 or EDC16C41 image whose checksum already validates.
3. Flash page Preflight button. Proceed is refused in the UI until that report is ready. The command does not unlock or write.
4. Bench script `preflight` and CLI `python3 python/ecu_scripting.py preflight dump.bin`.
5. Self-check reports `preflight_ok`. Empty input is not ready.

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
