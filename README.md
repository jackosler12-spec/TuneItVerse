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
python3 python/ecu_scripting.py logsummary path/to/log.csv
python3 python/ecu_scripting.py dtc P0087
python3 python/ecu_scripting.py strings path/to/dump.bin
```

Scripts page runs the same ideas against the loaded BIN (`identify`, `checksum`, `correct`, `compare`, `seedkey`, `poke`, `tables`, `maplog`, `hex`, `profile`, `report`). No eval, no shell. Scripts → Self-check runs the offline operational check. Maps → Import definition pack loads hint JSON and cannot enable write.

Data Logging can import a CSV, replay it, and build a clamped STFT/LTFT multiplier preview. Compare lists changed ranges against the working BIN. Neither path writes a calibration.

## Current status (v3.43.0)

Honest summary — see [COMPLETION.md](COMPLETION.md) for the full log.

**Operational offline:** BIN/XDF (text)/A2L load and edit, table grid + 3D orbit view, undo/redo, copy/paste, interpolate/smooth, BIN compare, offline DTC explain, CSV log import/export/replay, bench scripts (including strings extraction), P01 CS1 + CVN, GM 2-byte seed/key (algo tables). Binary/encrypted TunerPro XDFs get a clear error.

**P01_0411 (OS 12225074):** FullImage backup protocol is implemented from the PcmHammer read log + Kernel-P01.bin (scripted fake-PCM tests green; STN link and hardware untested). Experimental kernel calibration write (erase+write+CRC) is behind a Flash checkbox and the typed phrase `WRITE CALIBRATION TO PCM` (OS 12225074 / Intel 28F400B only). Off by default. Protocol from public PcmHammer host source used as spec only. **Not bench-tested.**

**EDC16C41:** Identify + measured 4-byte seed/key. Checksum is **report-only** (RSA-like 136-byte signature block at 0x1FDF78 detected on 392203 images). No flash segment, no write.

**Everything else in the catalog:** identify / report-only. No live write. Hardware paths (ELM/J2534/STN) are not bench-tested on this tree.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
