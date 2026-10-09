# TuneItVerse

TuneItVerse (JRTuners) — Open ECU tuning platform. Free alternative to expensive commercial tools.

TuneItVerse is a desktop ECU tuning and diagnostics application: a Tauri (Rust) backend with a vanilla HTML/JavaScript front-end that talks to vehicle ECUs (CAN, VPW, ISO-TP, serial/ELM327, J2534) to log live data, read/clear DTCs, inspect/patch BIN/XDF files, and run a guided flash pipeline with fail-closed safety gates.

## Stack
- Rust 2021 (Tauri backend) + vanilla JS/HTML/CSS
- serialport, serde, quick-xml, libloading (J2534), sha2
- Legacy C# / XML / XDF / BIN reference assets in `reference/`

## Run
```bash
npm install
npm run dev      # Tauri dev
npm run build    # release desktop binary + copy TuneItVerse.exe to this folder
```

After every release build, `TuneItVerse.exe` is copied to the repository root (this folder). Double-click that file to run the current version.

Bench helper for personal dumps:
```bash
python3 python/ecu_scripting.py identify path/to/dump.bin
python3 python/ecu_scripting.py checksum path/to/dump.bin
python3 python/ecu_scripting.py seedkey P01_0411 1234 1
python3 python/ecu_scripting.py diff stock.bin tuned.bin
python3 python/ecu_scripting.py adapters
python3 python/ecu_scripting.py profile path/to/dump.bin
python3 python/ecu_scripting.py hex path/to/dump.bin 0 64
python3 python/ecu_scripting.py report path/to/dump.bin
python3 python/ecu_scripting.py diff stock.bin tuned.bin
python3 python/ecu_scripting.py logsummary path/to/log.csv
python3 python/ecu_scripting.py dtc P0087
python3 python/ecu_scripting.py readiness path/to/dump.bin
```

Scripts page runs the same ideas against the loaded BIN (`identify`, `checksum`, `correct`, `compare`, `seedkey`, `poke`, `tables`, `maplog`, `hex`, `profile`, `report`). No eval, no shell. Scripts → Self-check runs the offline operational check. Maps → Import definition pack loads hint JSON and cannot enable write.

Data Logging can import a CSV, replay it, and build a clamped STFT/LTFT multiplier preview. Compare lists changed ranges against the working BIN. Neither path writes a calibration.

## Current status (v3.39.0)

v3.39.0 loads the previously unwired v3.33 seed/adapter desk and v3.34 script runner. Flash → Readiness is an offline identify/checksum checklist (`flash_allowed_now` stays false). Seed coverage lists catalog families with no dump-derived pairs. Maps → CSV exports the open table. Write path remains P01_0411 and EDC16C41.

See [COMPLETION.md](COMPLETION.md) and [PRODUCT.md](PRODUCT.md). Catalog includes P01, P59, EDC16C41, EDC15/17, ME7/9, MED17, Delphi DCM, SID803, Honda Keihin, Simos 18, Trionic 8, Transtron 4HK1. Holden/GM P01 BINs auto-load the Universal Patcher TableSeek pack. Offline BIN/XDF/A2L edit and save. Dashboard Write column and identify write_allowed flag match the live path (P01_0411 and EDC16C41 only). P59/Honda checksum correction and write stay blocked.



Diagnostics can explain a typed SAE code offline (`P0301`, `P0087`). Maps can hold a reference BIN and report cell deltas for the selected table. Clamp, percent, table redo, and BIN undo/redo stay local until you save. None of those flip write_allowed.

Inspect page reviews a loaded dump: hex+ASCII window, block entropy, markdown tune report. None of those flip write_allowed.

GM 2-byte security-access tables (`reference/2byte-keys.txt`) run offline: seed + algo index → key. Measured pairs in `seed_tables.json` win when present. Licensed 5-byte GM keys are not included.

Live: Mode 01 / VIN / DTCs on ELM ASCII for CAN/UDS; raw VPW only when you pick VPW. J2534 shows Connected when PassThru is open.

Flash: identify → voltage gate → backup → unlock → write, with live `flash-progress` events. Honda and P59 writes stay blocked. Only P01_0411 and EDC16C41 advertise a live write path.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
