# TuneItVerse v3.58.0 — shop workflow

## v3.58.0 changes

- STFT preview lists proposed cells and leaves the working table alone. Accept cell changes the one selected cell in the editor. The ECU is not written. Apply Patch is still the BIN write.
- A Live Dashboard hit cell selects that cell on the open Maps table. If no table is open, the status says so.
- Cancel on backup, Nissan scan, and guided flash stops before the next step. The result is not a success. A cancelled backup is not saved. Cancel before erase does not touch calibration. Cancel after erase uses the existing recovery erase and `success: false`.
- Export / Import is the same workspace JSON, now with stock, tune, definition text, log CSV, notes, and the identify capability snapshot. `ecu_write` on the file is false. Old exports that lack those keys still import.
- `installer/TuneItVerse.iss` installs the exe for x64 or i686. `scripts/sign-exe.ps1` signs only when `WINDOWS_PFX_BASE64` is set, and exits 2 otherwise. CI artifact names stay `unsigned` unless `SIGN_RELEASES` is true. No certificate is in the repo, so this release is unsigned.
- Log studio Explain cites logged rows (index, time, channel min/max/mean) and states that it does not emit a calibration value.
- **Not hardware-tested.** No adapter and no vehicle were used.

# TuneItVerse v3.57.0 — replay fixture bench

## v3.57.0 changes

- Connect has Replay fixture. It loads a checked-in ELM trace through `replay_port` and sets the session protocol to `replay`. The status line is `Replay fixture (not a vehicle)`.
- Flash write, unlock, and DTC clear are refused while that session is open. They do not send those commands to the script.
- The decode test uses the same fixture frames the parser tests already expect: Section A1 CALID `0VS43B`, Section C support PIDs, and ECT 83.0.
- **Not hardware-tested.** No adapter and no vehicle were used.

# TuneItVerse v3.56.0 — definition coverage and tune change report

## v3.56.0 changes

- Maps status shows a coverage line: editable, read-only (`patch_blocked`), and skipped, with the skip reasons the XDF reader already produced. It does not add map addresses.
- TunerPro `<XDFFORMAT>` files now load their table list from `parse_xdf_definitions`. The report command only returns a count, so the old path treated a successful parse as "no tables".
- Compare has Change report and Export report. The report lists changed ranges, a short old/new hex preview, and the name of any loaded table whose bytes overlap that range. Export uses the existing text save dialog. `write_allowed` stays false.
- **Not hardware-tested.** Tests use four-byte fixtures and synthetic table metadata.

# TuneItVerse v3.55.0 — fail-closed DTC read and live verify

## v3.55.0 changes

- `read_dtcs_cmd` and `read_freeze_frame_cmd` return an error when no adapter is connected or the read fails. They no longer answer with an empty code list or `{}`.
- `verify_after_write` returns an error when disconnected, when the probe fails, or when the live read-back does not match. The success string is only `matched=true`.
- `clear_dtcs_cmd` is unchanged: offline clear stays `success: false`.
- Diagnostics UI treats a missing code list as an error instead of "No DTCs reported."
- **Not hardware-tested.** The new tests call the commands with no port, and check the match/mismatch formatter. No ECU was read.

## v3.54.0 — 32-bit (i686) Windows CI build for J2534 drivers

- CI `rust` job is now a matrix: `x86_64-pc-windows-msvc` and `i686-pc-windows-msvc`. Both run `cargo check --locked`, `cargo test --lib` and `cargo build --release`.
- Each target uploads an **unsigned** exe artifact (`TuneItVerse-<target>-unsigned`, kept 14 days). The i686 exe is the one to use with 32-bit-only J2534 vendor DLLs (most pass-thru drivers); the v3.53 PE arch check reports which build can load which DLL.
- **Not verified:** neither exe has been run against a real J2534 device. These are raw `cargo build` exes, not Tauri MSI/NSIS installers, and are not code-signed. Flash write gates unchanged; no new write paths.

## v3.53.0 changes (2026-11-04)

- **J2534 registry discovery:** `j2534_list.rs` now parses `HKLM\SOFTWARE\PassThruSupport.04.04` (native + WOW6432Node) into structured entries: name, vendor, FunctionLibrary, non-zero protocol flags (CAN, ISO15765, J1850VPW, …). The DLL's PE header is read to report x86/x64/arm64 and whether this build can load it (a 32-bit driver cannot load into the 64-bit app; this is reported, not hidden). New command `j2534_list_devices_detailed`; the Connect page fills the DLL path from the first loadable entry.
- `j2534_connect` with no path now tries loadable registry DLLs first, then the old hardcoded names.
- Uses stock `reg.exe` (no new crate), Windows-only via `cfg`; Linux/macOS return an empty list with a note.
- **Non-blocking I/O:** 52 adapter/ECU commands (serial, STN, J2534, DTC, unlock, logging capture, P01 backup, guided pipeline, verify) are now `#[tauri::command(async)]`, so they run off the main thread and long transfers no longer freeze the window. The adapter mutexes still serialise access; no protocol logic changed.
- **Not verified:** registry parsing is tested on a synthetic `reg query` sample only; no real Windows machine or J2534 device was used. No cancel button for long transfers yet; no progress events beyond those already emitted. No new write paths.

## v3.51.0 changes

- New **Live Dashboard** view (sidebar, after Data Logging). Read-only; it never writes to a BIN or an ECU.
  - Gauges for up to 12 logged channels (last value, min/max/count), a multi-channel time series of the last 300 samples, an X/Y scatter and a hits-per-cell grid.
  - Cell binning uses the open Maps table's numeric row/column headers (nearest breakpoint). Samples outside the axis by more than half a step are counted as `outside_axis`, not piled into the edge cell. Non-numeric headers mean scatter only.
  - An optional third channel shows its mean per cell (e.g. STFT vs RPM/MAP) as a preview only; nothing is applied to the table.
- Backend: `live_dashboard.rs` / `dashboard_snapshot` reads the existing logger buffer (live Mode 01, imported CSV, or demo samples, which are flagged in the status line). Channel names come straight from the log; Nissan DIDs stay raw with no invented meanings.
- 5 new Rust unit tests (binning, out-of-range, descending axes, fail-closed axis parse, demo flag). Frontend check covers the new script.
- **Not hardware-tested.** Only tested against synthetic samples; the live feed is the same Mode 01 poll as Data Logging (2 Hz dashboard refresh while the view is open).
- Still missing: no alarms/thresholds, no user-saved gauge layouts, no live cursor on the Maps grid itself, no unit conversion, gauge ranges are the observed min/max (not sensor limits).

# TuneItVerse v3.50.0 — TunerPro XDF: float cells, strided layouts, embedded axes

## v3.50.0 changes
- **Float cells**: XDF items with `mmedtypeflags` 0x10000 (or header `float="1"`) and 32-bit cells are read and written as IEEE-754 single precision, honouring LSB-first and column-major. Convention taken from bri3d/a2l2xdf (`FLOAT32_IEEE` → flag 0x10000, 32 bits). Patches still need linear math and the exact shape; NaN/inf or values outside f32 range are refused and the BIN is left untouched. 64-bit floats are skipped and listed.
- **Embedded axes**: X/Y `XDFAXIS` blocks with their own `EMBEDDEDDATA` address are read from the BIN (int or float, own scaling) and returned as `axes_x`/`axes_y`; the editor uses them as row/column headers when the definition has no labels. Axes are read-only. An axis whose cell count does not match the table, or whose layout is unknown, is dropped and listed in the skip report; the table itself is kept.
- **Strided layouts**: `mmedmajorstridebits` equal to the cell size (or 0) is contiguous, as a2l2xdf writes it. A larger whole-byte major stride is read as the step between consecutive cells in storage order. TunerPro does not document this, so those tables are **read-only** (`patch_blocked`), and the reason is shown in the editor. A non-zero minor stride has no public definition and is still skipped.
- Editor side panel shows layout, axis source and whether editing is blocked; extract notes are shown in the table status.
- **Not checked against TunerPro itself**: tests use synthetic XDF snippets only, no real definition with float/strided/axis data was available in the repo. No flash/write paths changed.

# TuneItVerse v3.49.1 — VPW send path: J1850 CRC-8 + ELM text framing

## v3.49.1 changes
- **VPW TX check byte**: every `vpw.rs` request builder now appends the SAE J1850 CRC-8 (the same routine the receive path already used, verified on real fixture frames such as `486B104100BFBFF990` → `B5`) instead of an additive sum. `vpw::validate_frame` now accepts CRC-8 only.
- **No raw binary to ELM/STN adapters**: `vpw::request_response` now picks the transport from the session. On ELM327/STN it checks `ATDPN` reports protocol 2 (J1850 VPW), sends `ATSH <3-byte header>` plus the data bytes as hex (the adapter adds the CRC), parses the reply with the J1850 header parser, and restores the ELM default functional header `68 6A F1` on every exit. It refuses requests over 7 data bytes, the ELM327 J1850 limit; those need STN `STPX`, which the P01 backup already uses. Binary frames go only to raw-VPW interfaces, and `send_frame` refuses any frame without a valid CRC-8.
- This affects live verify (Mode 3C) and the raw-VPW Mode 01/09 path. No new write paths, and the experimental P01 write is unchanged and off by default.
- **Not hardware-tested**: the vectors come from replayed fixture frames only. The ISO 9141-style frames (`68 6A F1`, additive checksum) in `security.rs`/`dtc.rs` and the 10400-baud probe in `commands_p1.rs` were not changed. ISO 9141 legitimately uses an additive checksum, but whether those paths are labelled and routed correctly is still unverified.


## v3.49.0 changes

### Changes
- **Table CSV export/import**: Maps page now has working Export CSV and Import CSV buttons. Export writes row,col,value for the current table. Import applies matching cells back into the grid (with undo). Completes the feature announced in v3.48.0.
- Version synced to 3.49.0 across package.json, Cargo.toml, tauri.conf.json.
- Docs refreshed. No change to write paths, checksum correctors, or hardware status. Everything remains honest: hardware_tested false, P01 write experimental and not bench-tested, EDC16 checksum report-only.

### Still missing for full industry-leading operational status
- Bench-tested hardware paths (ELM/J2534/STN on real ECUs).
- Verified checksum correctors and FullImage read/write for families beyond P01.
- Measured seed/key tables for additional ECUs (user dumps only).
- Full async I/O for long transfers.
- BDM/JTAG/bench mode.
- Driver plugin SDK.
- Community-sourced map definitions with citations for more families.

Offline BIN/XDF/A2L/logging/scripts remain fully operational. Build your own.

# TuneItVerse v3.48.0 — table CSV export + polish

## v3.48.0 changes

### Changes
- **Table export**: Maps page grid now has an "Export CSV" button. Exports the current table values (row,col,value) to a native save dialog. Useful for offline analysis or sharing parameter sets without full BIN.
- Version synced to 3.48.0 across package.json, Cargo.toml, tauri.conf.json, installer.
- Docs refreshed. No change to write paths, checksum correctors, or hardware status. Everything remains honest: hardware_tested false, P01 write experimental and not bench-tested, EDC16 checksum report-only.

### Still missing for full industry-leading operational status
- Bench-tested hardware paths (ELM/J2534/STN on real ECUs).
- Verified checksum correctors and FullImage read/write for families beyond P01.
- Measured seed/key tables for additional ECUs (user dumps only).
- Full async I/O for long transfers.
- BDM/JTAG/bench mode.
- Driver plugin SDK.
- Community-sourced map definitions with citations for more families.

Offline BIN/XDF/A2L/logging/scripts remain fully operational. Build your own.

# TuneItVerse v3.47.0 — capability matrix, sourced ECU DB, real TunerPro XDF parser, offline UX

## v3.47.0 changes (2026-10-21)

### 1. ECU database (index 1.26, 16 families)
- Every family now carries `capabilities` for `identify`, `read`, `checksum`, `seed_key`, `write`, `map_hints`. Each one has a `status` from a fixed enum (`verified_dump`, `verified_log`, `public_source`, `reference_doc`, `experimental`, `report_only`, `unverified`, `not_supported`) and a `source`. Every entry has `hardware_tested: false`.
- A schema-validated loader (`validate_entry` / `validate_index`) drops invalid entries and reports them in Self-check. Tests enforce source and status on every capability. They also enforce that only `P01_0411` has a checksum corrector and that nothing is marked hardware-tested.
- **GM_P59 corrected:** it is 1 MiB, not 512 KiB (PcmHammer `PcmInfo.cs` @033cb03). Unsourced map addresses and OS IDs were removed.
- **New families GM_P10 and GM_P12:** identify-only, sourced from PcmHammer `PcmInfo.cs`. Their size and key-algorithm index are public source. There is no checksum, read or write.
- **New `gm_osid_map.json`:** 2618 GM OS IDs mapped to a PCM type, generated by `scripts/gen-gm-osid-map.mjs` from the same pinned `PcmInfo.cs`. `identify_bin` now reports the OS ID, the PCM type and the source.
- P59 OS IDs `12579405` and `12587603` were wrongly listed as P01 markers. They are removed. The P01 checksum corrector now refuses any OS ID the map lists as a non-P01 GM type.
- `identify_bin` no longer reports EDC16C41 as `correction_safe` or `write_allowed`. `correction_safe` is true only for corrector families, and `write_allowed` follows the live write gate.
- Size-catalog map hints that have no citation are labelled `UNVERIFIED hint (no cited source)`.

### 2. Offline tools
- **Real TunerPro XDF parser** (`parse_tunerpro_xdf`, `parse_xdf_report`), checked against TunerPro help and RomRaider's XDF conversion layer:
  - Reads XDFTABLE and XDFCONSTANT.
  - Subtracts BASEOFFSET when `subtract` is set.
  - Reads 1-based CATEGORYMEM.
  - Handles the `mmedtypeflags` signed, LSB-first and column-major bits.
  - Uses axis LABELs as headers.
  - Float cells, strided layouts, unsupported cell sizes and items with no address are **skipped and listed**. The Maps page shows the skipped count. They are never guessed.
- **Scaling math evaluator** for `+ - * / ( )`, unary minus and numbers. Patches are written only when the equation is verified linear. Non-linear math, ragged grids and shape mismatches are refused. The old path silently wrote physical values as raw.
- The TableData parser no longer invents a fallback address (`0x8000+i*0x100`) for tables that have none.
- BIN workflow:
  - Correct CS and script results now reach Save BIN (`setWorkingBin`). Before this, overlays wrote a `window.currentBin` that Save never read.
  - Unsaved-changes guard on both the window close (native dialog) and reload.
  - Recent files (bin/xdf/a2l/csv; extension and 16 MiB checks).
  - Cells are highlighted against the loaded BIN and the reference BIN.
  - Patching a table marked UNVERIFIED asks for confirmation.
- Lost controls are restored in `index.html`: reference/cell delta, BIN undo/redo, smooth/interp/copy/paste/clamp/%/STFT, hex poke, flash Compare/Verify/write gates, and the log studio panel.

### 3. Identification and checksums
- P01 CS1/CS2 stay the only corrected checksums (verified on the reference image and on the private dump through `TIV_P01_BIN`).
- EDC16C41 stays report-only.
- Non-P01 GM images are refused by the P01 corrector, with the PCM type named in the message.

### 4. Capability surface
- New **Capabilities** view and a per-BIN capability strip, driven by the `family_capabilities` command.
- The flash catalog labels P01 write "experimental write (off by default, not bench-tested)". The old 'LIVE' label is gone.
- **No new write path.** The experimental P01 kernel write remains behind its existing flag and typed confirm.

### 5. UX
- New Inspect and Compare views, plus a status bar (family / OS / size / dirty / version).
- Keyboard shortcuts:
  - Ctrl+O open, Ctrl+S save.
  - Ctrl+Alt+Z / Y undo / redo.
  - Alt+1..0 switch pages.
  - Ctrl+F filter tables.
  - F1 or `?` shows help.
- `npm test` now runs `scripts/check-frontend.mjs`. It checks script syntax and tags, that every invoked command is registered in Rust, that every bound button id exists, that ids are unique, that nav panels exist, and that version strings agree.

**Tests:** `cargo test --lib` 329 passed (304 at v3.46.1). They also pass with `TIV_P01_BIN` set to a private P01 dump. `npm test` passes.

**Unverified in v3.47.0:**
- Nothing in this release was bench-tested and no hardware path was exercised.
- P10/P12/P59 are identify-only.
- XDF: 64-bit float cells and non-zero minor strides are skipped; strided tables and embedded axes are read-only; axis editing, non-linear math patching, XDF patches/flags/functions are not supported (v3.50.0).
- Catalog map hints stay unverified.
- Nissan DID meanings stay unknown.
- The raw-VPW send path was wrong in v3.47.0 (fixed in v3.49.1, still not hardware-tested).
- No Windows exe was built for this change (the work was done on Linux; CI builds and tests on Windows).

## What is / is not operational

| Area | Status |
|------|--------|
| Offline BIN / TunerPro text XDF / A2L inspection, table grid + 3D, undo/redo, interpolate, compare | Operational offline. XDF 32-bit float cells read/write; embedded axes and strided tables read-only; non-linear patches refused (v3.50.0). |
| Per-family capability matrix | Operational. Every status has a cited source; `hardware_tested` is false everywhere. |
| P01 FullImage backup protocol | Implemented (fake-PCM tested). STN link and real PCM untested. |
| P01 checksum (CS1/CS2) | Verified on the reference image and a private dump. Corrector refuses non-P01 GM OS IDs. |
| P01 guided write | Experimental kernel cal write behind a flag + typed confirm (12225074 / 28F400B). Off by default. Not bench-tested. |
| GM P59 / P10 / P12 | Identify only (PcmHammer PcmInfo.cs). No checksum, read or write. |
| EDC16C41 checksum | Report-only (signature). |
| EDC16C41 write | Off. |
| Other catalog families | Identify / report-only / unverified map hints. No write. |
| CSV log import/export/replay, bench scripts, DTC explain | Operational offline |
| ELM/OBDLink Mode 01 live data + logging | Parser, support masks and timing fixed; tested on replayed real Patrol frames. Not run on a live car from this build. |
| Nissan CAN read-only scan + raw DID logging | Implemented from observed frames (3-DID reads, single fallback); ID meanings unknown. |
| Hardware (ELM / J2534 / STN) | Code paths exist; not bench-tested on this tree. |

## Current operational status

- Full offline workflow for BIN editing, map definition (XDF/A2L/catalog), checksum report (P01 correctable), seed/key calculation (known algos), logging analysis, comparison, scripts.
- Guided flash gates stay fail-closed. After a verified FullImage backup, P01 write stays off unless `experimental_p01_write` is set and the typed confirm phrase matches. No other family has a write path. Experimental write is not bench-tested.
- Safety: voltage gates, audit log, fail-closed on unknown family / bad checksum / no backup.

Never flash without a verified backup and stable power. Personal dumps only.

## Still needs your bench

1. Measured P59/P10/P12 checksum details and kernels. These stay identify-only.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk (v3.53 structured discovery + PE arch check) are live.
5. Bench-test the experimental P01 kernel cal write on your STN + PCM (protocol from public PcmHammer host source; still hardware-unverified). FullImage backup already exists.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Build your own. No bullshit prices.
