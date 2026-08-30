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

## Current status (v3.2.0 slice 1)

See [COMPLETION.md](COMPLETION.md). Identify now lists every family that shares a BIN size. 512 KB P01 checksum correction and fail-closed risk default are implemented locally and still need the follow-up commit of `checksum.rs` / `flash.rs` / `src/main.js`.

Working core on main: serial connect, DTC 03/07/0A, live Mode 01 PIDs + logger, XDF parse/patch, P01 128 KB + EDC16 checksums, guided flash only when connected, J2534 surface, 5-family ECU DB, BIN identify/compare/map-from-log.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
