# TuneItVerse

TuneItVerse (JRTuners) — Industry-leading open ECU tuning platform. Free alternative to expensive commercial tools.

TuneItVerse is a desktop ECU tuning and diagnostics application: a Tauri (Rust) backend with a vanilla HTML/JavaScript front-end that communicates with vehicle ECUs (CAN, VPW, ISO‑TP, serial/ELM327, J2534 and FTDI-based devices) to read/record live data, run diagnostics (DTCs/OBD2), convert and inspect binary ECU files, and perform guided flashing workflows. The repository contains the Tauri app, Rust device/protocol implementations, and a large reference set (XML/XDF/bin/databases) used by the tuner tools.

## Quick summary / Stack
- Language(s): C# (legacy/reference tools and data), Rust (Tauri backend, ECU protocols), JavaScript/HTML/CSS (front-end UI)
- Framework / runtime: Tauri (desktop), Rust 2021 edition
- Notable libraries: serialport (Rust), tauri-plugin-fs, tauri-plugin-dialog, serde + serde_json, quick-xml, libloading (J2534)

## How it's organized
```
README.md                Project readme (this file)
package.json             Frontend + tauri npm scripts (v1.0.0)
src/                     Web UI (index.html, main.js, styles.css)
src-tauri/               Tauri (Rust) backend: ECU protocols, device drivers, tauri config
  Cargo.toml
  src/                   can.rs, dtc.rs, flash.rs, security.rs, xdf.rs, j2534.rs, lib.rs, main.rs, etc.
reference/               Large collection of legacy C# source, DLLs, XML/XDF/bin databases and example ECU files + ecu_database/
.github/                 GitHub workflow/metadata
```
How it fits together: The UI in `src/` provides the desktop interface. The Tauri Rust code in `src-tauri/` exposes native functionality (serial/CAN communication, file I/O, checksum/security algorithms, XDF parsing, flashing helpers, J2534) to the frontend via Tauri commands. The `reference/` folder stores legacy C# tools, device drivers, and many ECU mapping and binary example files used by the tools or for development/testing.

## How to run (development)
Prerequisites:
- Node.js (16+ recommended) and npm
- Rust toolchain (stable) and cargo
- Tauri prerequisites for your OS (see https://tauri.app for platform-specific requirements: on Linux you may need build-essential, libgtk-3-dev, etc.)
- On Linux you may need udev rules or root permissions to access serial/USB devices (FTDI/J2534).

Commands (from repository root):

```bash
# Install JS dependencies
npm install

# Run the app in development mode (hot-reload front-end + tauri dev)
npm run dev

# Build a release desktop binary
npm run build
```

Notes:
- `npm run dev` runs the Tauri dev flow and will build the Rust backend on first run. If you prefer, you can build the backend directly from `src-tauri/` with `cargo build` or `cargo build --release`.
- If you use J2534 or vendor drivers you may need platform-specific drivers (J2534 DLLs on Windows) — see the `reference/` folder for examples and DLLs included for testing.

## Usage (high-level)
- Connect a supported device (ELM327-style adapter for OBD-II serial, FTDI/serial devices, or J2534-capable interfaces).
- Start the app and select the device/port in the UI.
- Use Logger to capture live PIDs and CAN/ECU traffic.
- Use the DTC / OBD features to read/clear diagnostic trouble codes.
- Use the Flash / ECU tools for guided flashing — consult `reference/guided_flashing_pipeline.md` for recommended flashing steps and safety checks.
- The `reference/` directory contains many sample XDF, XML, BIN and DLL files used by the platform for decoding and verification. ECU definitions live in `reference/ecu_database/`.

## Contributing / Next steps
- This repo combines a new Tauri/Rust implementation with legacy C# reference assets. If you're adding device support, prefer contributing Rust-side implementations in `src-tauri/src/`.
- Please add tests or sample logs when adding protocol changes.
- Expand `reference/ecu_database/` with your own verified dumps and map addresses.

## Current status

See [COMPLETION.md](COMPLETION.md) for the full honest feature matrix.

**v1.0.0 Fully operational for core DIY + pro workflows:** serial connect, DTC diagnostics, live Mode-01 PIDs, P01 table auto-load from real XML addresses, EDC16/EDC17/MED17 community maps (family-aware), checksum correct (P01 + multipoint), guided flash Mode 34/36/37 with backup/kernel/verify + UDS ready, ECU DB (P01, EDC16C41, P59, MED17, EDC17 fully refined), J2534 production path (list/connect/write/read), native table editor (grid/3D/hex).

**Expand next:** more map editors, additional ECU families, full UDS multi-frame refinements, hardware validation on your bench.

### CI

Push/PR to `main` runs `.github/workflows/ci.yml` (`cargo check`, `cargo test --lib`, `npm test` sanity).

## License & contact
MIT License — see [LICENSE](LICENSE). Project contact: JRTuners / repository maintainers. Build your own tools — no bullshit prices.
