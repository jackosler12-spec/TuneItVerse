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

**v3.49.0**: Maps page has working Table CSV export and import. Offline tools remain fully operational. Hardware paths still untested.

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
```

Scripts page runs the same ideas against the loaded BIN (`identify`, `checksum`, `correct`, `compare`, `seedkey`, `poke`, `tables`, `maplog`, `hex`, `profile`, `report`). No eval, no shell. Scripts → Self-check runs the offline operational check. Maps → Import definition pack loads hint JSON and cannot enable write.

Live data over ELM327/OBDLink reads the ECU's Mode 01 support masks first and polls only the PIDs it lists (incl. SAE 0x23 rail pressure); replies are parsed per line with the CAN/J1850 header stripped. A read-only Nissan CAN scan (ATSH7E0, 10C0, 22xxxx support masks, 1081) lists supported IDs with unknown meanings. These paths are tested against replayed real OBDLink MX+ frames from a 2007 Patrol ZD30CRD (`reference/fixtures/`), not against a live car from this build.

Data Logging can import a CSV (including Car Scanner's long `SECONDS;PID;VALUE` export; GPS columns are dropped), replay it, and cite logged STFT/LTFT percent. That report does not emit a calibration value. Compare lists changed ranges, and a three-way report classifies stock, the working tune, and a third dump. None of these paths writes a calibration.

## Current status (v3.60.0)

- v3.60.0: Value sheet lists extracted cell changes for every loaded table. An address that does not resolve is not compared, and no placeholder number is shown. Cell delta uses the same refusal. Not hardware-tested. Installers stay unsigned unless a certificate is configured.
- v3.59.0: Trim report cites logged trim percent and does not emit a multiplier. Map-from-log names the mean RPM/MAP bin `mean_cell`. Flash and live-compare errors stay in their own words instead of a `Not connected:` prefix. Compare prints a three-way stock / tune / other report and does not patch a BIN. Not hardware-tested. Installers stay unsigned unless a certificate is configured.
- v3.58.0: STFT accept is one cell at a time. Dashboard hit cells select the open map cell. Backup, Nissan scan, and guided flash can be cancelled, and a cancel is not a success. Workspace export/import keeps stock, tune, definition, log, notes, and the capability snapshot in the same JSON file. The log explainer cites rows and does not emit a calibration value. Installers can be signed only when a certificate is configured; this release is unsigned. Not hardware-tested.
- v3.57.0: Connect can open a checked-in ELM trace as a replay fixture. Status says replay, not a vehicle. Flash write, unlock, and DTC clear stay refused. Not hardware-tested.
- v3.56.0: Maps shows definition coverage (editable, read-only, skipped). Compare writes a change report with table names and exports it as text. Neither path writes a BIN. Not hardware-tested.
- v3.55.0: DTC read, freeze frame, and live verify fail closed. A disconnected adapter is an error, not an empty code list or a soft "verify note". Not hardware-tested.
- v3.51.0: Live Dashboard view — gauges, time-series chart, X/Y scatter and hits-per-map-cell grid from the logger buffer (Mode 01 / imported CSV). Read-only, not hardware-tested.


This is the honest summary. [COMPLETION.md](COMPLETION.md) has the full log, and the in-app **Capabilities** page shows the per-family matrix with a source for every status.

**Operational offline:**
- BIN, text TunerPro XDF and A2L load and edit. The XDF reader supports BASEOFFSET, categories, signed / LSB-first / column-major cells and linear scaling math. 32-bit float cells are read and edited; embedded X/Y axes are read from the BIN; strided tables are read-only (layout interpretation unverified). Non-linear math, 64-bit floats and minor strides are refused or skipped and listed.
- Table grid with 3D orbit view, undo/redo, copy/paste, interpolate/smooth, and diff highlight against the loaded and reference BIN. **New: Export current table to CSV.**
- BIN compare.
- Recent files and an unsaved-changes guard.
- Offline DTC explain.
- CSV log import/export/replay.
- Bench scripts.
- P01 CS1/CS2 + CVN.
- GM 2-byte seed/key (algorithm tables).
- Binary or encrypted TunerPro XDFs get a clear error.

**P01_0411 (OS 12225074):**
- The FullImage backup protocol is implemented from the PcmHammer read log + Kernel-P01.bin. Scripted fake-PCM tests pass; the STN link and hardware are untested.
- The experimental kernel calibration write (erase+write+CRC) is off by default. It sits behind a Flash checkbox and the typed phrase `WRITE CALIBRATION TO PCM`, and is limited to OS 12225074 / Intel 28F400B.
- The protocol comes from public PcmHammer host source, used as a spec only. **Not bench-tested.**

**GM P59 / P10 / P12:** identify only, from PcmHammer `PcmInfo.cs`: size, key-algorithm index, and a 2618-entry OS ID → PCM type map. The P01 checksum corrector refuses these images.

**EDC16C41:** Identify + measured 4-byte seed/key. Checksum is **report-only**: an RSA-like 136-byte signature block at 0x1FDF78 is detected on 392203 images. No flash segment, no write.

**Everything else in the catalog:** identify / report-only / unverified map hints. No write. Hardware paths (ELM/J2534/STN) are not bench-tested on this tree.

Tests: `cargo test --lib` from `src-tauri/`, and `npm test` (frontend consistency check). Set `TIV_P01_BIN=/path/to/your/p01.bin` to also run the private-dump checksum tests; the dump is never committed.

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
