# TuneItVerse v3.38.0 — operational desk

v3.37 log studio stays. This pass closes the gap between "I have a dump" and "is this image actually allowed to go near flash".

## What this pass changed

1. Version **3.38.0** across crate / package / Tauri / installer / HTML.
2. Flash page **Preflight**. Command: `flash_preflight_cmd`. Checks family, OS confirmation, checksum, and the 12.5 V gate. Ready stays false unless the live write flag is already true.
3. Inspect **Strings**. Command: `bin_strings_cmd`. Printable runs only. Not an identification.
4. Diagnostics **Lookup**. Command: `dtc_lookup_cmd`. Generic SAE descriptions. Does not clear codes.
5. Bench CLI: `strings`, `preflight`, `dtc`. CLI preflight cannot advertise write.
6. Self-check reports `ops_desk_ok`.

Write path remains **P01_0411** and **EDC16C41** only. Preflight cannot enable write.

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
