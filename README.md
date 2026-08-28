# TuneItVerse

TuneItVerse (JRTuners) — Open ECU tuning platform. Free alternative to expensive commercial tools.

TuneItVerse is a desktop ECU tuning and diagnostics application: a Tauri (Rust) backend with a vanilla HTML/JavaScript front-end that talks to vehicle ECUs (CAN, VPW, ISO-TP, serial/ELM327, J2534) to log live data, read/clear DTCs, inspect/patch BIN/XDF files, and run a guided flash pipeline with fail-closed safety gates.

## Stack
- Rust 2021 (Tauri backend) + vanilla JS/HTML/CSS
- serialport, serde, quick-xml, libloading (J2534)
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
```

## Current status (v3.0.0)

See [COMPLETION.md](COMPLETION.md). That file is the honest matrix — v2.9 docs claimed identify/512KB CS/fail-closed flash were wired; some of that lived only in `v29_tools.rs` / the UI until this pass.

Working core: serial connect, DTC 03/07/0A, live Mode 01 PIDs + logger, XDF parse/patch, P01 + EDC16 checksums, guided flash when *connected*, J2534 surface, 5-family ECU DB, BIN identify/compare/map-from-log helpers.

Still needs your bench: EDC17/MED17 seed tables from your dumps, Windows J2534 registry enum, embedded Python, hardware validation of 512 KB P01 CS vs PCM Hammer.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
