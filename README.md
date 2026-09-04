# TuneItVerse

TuneItVerse (JRTuners) — Open ECU tuning platform. Free alternative to expensive commercial tools.

TuneItVerse is a desktop ECU tuning and diagnostics application: a Tauri (Rust) backend with a vanilla HTML/JavaScript front-end that talks to vehicle ECUs (CAN, VPW, ISO-TP, serial/ELM327, J2534) to log live data, read/clear DTCs, inspect/patch BIN/XDF/A2L files, and run a guided flash pipeline with fail-closed safety gates.

## Stack
- Rust 2021 (Tauri backend) + vanilla JS/HTML/CSS
- serialport, serde, quick-xml, libloading (J2534), sha2
- Legacy C# / XML / XDF / BIN reference assets in `reference/`

## Run
```bash
npm install
npm run dev      # Tauri dev
npm run build    # release desktop binary
```

Bench helper for personal dumps:
```bash
python3 python/ecu_scripting.py identify path/to/dump.bin
python3 python/ecu_scripting.py checksum path/to/dump.bin
python3 python/ecu_scripting.py seedkey P01_0411 1234 1
```

## Current status (v3.7.0)

See [COMPLETION.md](COMPLETION.md). TunerPro XDFFORMAT and A2L CHARACTERISTIC parsers, case-insensitive table math, scale/offset/smooth, STFT preview from logs. Unknown BIN sizes report checksum status only — no invented corrector.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
