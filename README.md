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
```

## Current status (v3.14.0)

See [COMPLETION.md](COMPLETION.md).

Offline (no ECU): Load BIN, Load XDF/A2L, identify family, validate/correct checksums (P01 / EDC16 only), edit maps, Save BIN.

Live: Mode 01 / VIN / DTCs on ELM ASCII for CAN/UDS; raw VPW only when you pick VPW. J2534 shows Connected when PassThru is open. Dashboard lists the embedded ECU catalog.

Flash: identify → voltage gate → backup → unlock → write. Mid-transfer voltage abort on VPW chunks and J2534 UDS downloads. Live `flash-progress` events drive the UI bar. Honda and unverified P59 writes stay blocked.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
