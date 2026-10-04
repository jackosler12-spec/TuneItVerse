# TuneItVerse v3.37.0 — log studio and BIN diff

v3.36 inspect stays. This pass closes the datalog gap that was still a heatmap-only hint.

## What this pass changed

1. Version **3.37.0** across crate / package / Tauri / installer / HTML.
2. Data Logging: session summary, replay frame, trim preview. Commands: `log_session_summary`, `log_replay_frame`, `log_trim_suggestion`.
3. Compare page: markdown range report via `bin_diff_report`. Working BIN stays the Maps image.
4. Bench CLI: `diff` prints ranges; `logsummary` reads a CSV. No shell, no write flag.
5. Self-check reports `log_studio_ok`. Preview multiplier is clamped 0.85–1.15 and cannot enable write.

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
