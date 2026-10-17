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

## Current status (v3.44.0)

Honest summary — see [COMPLETION.md](COMPLETION.md) for the full log.

**Operational offline:** BIN/XDF (text)/A2L load and edit, table grid + 3D orbit view, undo/redo, copy/paste, interpolate/smooth, BIN compare, offline DTC explain, CSV log import/export/replay, bench scripts, P01 CS1 + CVN, GM 2-byte seed/key (algo tables). Binary/encrypted TunerPro XDFs get a clear error.

**P01_0411 (OS 12225074):** FullImage backup protocol is implemented from the PcmHammer read log + Kernel-P01.bin (scripted fake-PCM tests green; STN link and hardware untested). Experimental kernel calibration write (erase+write+CRC) is behind a Flash checkbox and the typed phrase `WRITE CALIBRATION TO PCM` (OS 12225074 / Intel 28F400B only). Off by default. Protocol from public PcmHammer host source used as spec only. **Not bench-tested.**

**EDC16C41:** Identify + measured 4-byte seed/key. Checksum is **report-only** (RSA-like 136-byte signature block at 0x1FDF78 detected on 392203 images). No flash segment, no write.

**Everything else in the catalog:** identify / report-only. No live write. Hardware paths (ELM/J2534/STN) are not bench-tested on this tree.

Dashboard now has an Operational Status panel (v3.44.0).

Build your own. No bullshit prices.

MIT — see [LICENSE](LICENSE).
