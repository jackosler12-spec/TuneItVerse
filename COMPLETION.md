# TuneItVerse v3.36.0 — fingerprint, log stats, tune project

v3.35 supply watch and definition packs stay. This pass closes the session-workflow gaps that were still software-only.

## What this pass changed

1. Version **3.36.0** across crate / package / Tauri / installer / HTML.
2. Family fingerprint scores (`score_fingerprint_cmd`). Size is a weak signal. OS/part strings outrank size. A score never grants write.
3. Log channel stats (`analyze_log_channels_cmd`): min / max / avg / stddev from captured or imported samples only. STFT, ECT, and battery flags are hints, not map writes.
4. Tune project file (`build_tune_project_cmd` / `load_tune_project_cmd`). Load ignores any `write_allowed` in the JSON. BIN bytes are not restored from the project.
5. BIN search (`search_bin_cmd`) for ASCII or even-length hex. Project page wires all four.
6. Bench CLI: `strings` and `logstats`.

Write path remains **P01_0411** and **EDC16C41** only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.
8. Windows release build of `TuneItVerse.exe` on your machine (`npm run build`). This pass does not commit a stale or fake exe.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
