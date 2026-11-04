# TuneItVerse audit

**Date:** 2026-11-04
**Tree:** `main` at `65ff65b` (v3.54.0)
**Scope:** source, docs, CI, and every remote branch. No protocol or UI behavior was changed in this pass.

TuneItVerse is a Windows Tauri app (Rust backend, vanilla HTML/JS) for offline BIN/XDF/A2L work, Mode 01 logging, and a fail-closed flash pipeline. It does not bench-test hardware in this tree. Every ECU family has `hardware_tested: false`. The only write path is an experimental P01_0411 kernel calibration write, off by default.

Paid suites (HP Tuners, EFI Live, WinOLS, ECM Titanium, TunerPro) win on licensed coverage and bench-proven flash. This audit does not propose flipping write flags to compete with them. The gap they leave is a single shop workflow plus a public capability matrix.

## What already works

- Offline BIN edit, text TunerPro XDF (32-bit float read/write, embedded axes and strided tables read-only), A2L, undo, compare, and table CSV round-trip. See `README.md` and `src-tauri/src/xdf.rs`.
- Capability matrix with a source on every status. `ecu_database.rs` rejects `hardware_tested: true` (around the `hardware_tested` check near line 245).
- P01 CS1/CS2 correction refuses non-P01 GM OS IDs. EDC16C41 checksum is report-only because of the signature block.
- Guided flash keeps the experimental P01 write behind `experimental_p01_write` and the typed phrase. See `src-tauri/src/flash.rs` (`P01_WRITE_NEEDS_FLAG`).
- Live Dashboard, J2534 registry discovery with a PE arch check, and async adapter commands (v3.51.0 through v3.54.0 in `COMPLETION.md`).
- CI (`.github/workflows/ci.yml`) runs `cargo check`, `cargo test --lib`, and an unsigned release build for `x86_64-pc-windows-msvc` and `i686-pc-windows-msvc`, plus `npm test`.
- `scripts/check-frontend.mjs` checks script syntax, that every invoked command is registered, that button ids exist, and that version strings agree.

## Flaws

### Docs disagree with the binary

- `README.md` "Current status" still says v3.51.0 (around line 43). `package.json`, `Cargo.toml`, `tauri.conf.json`, the installer, and `index.html` say 3.54.0.
- `PRODUCT.md` is dated 2026-10-21 and describes v3.35+.
- `COMPLETION.md` title is v3.51.0. The v3.54.0 section is below that title.
- `src-tauri/src/lib.rs` line 1 still says v3.39.0.
- `V2_ROADMAP.md` says v2.0 success criteria were achieved, then the 2026-10-21 note on the same page says that claim was premature.
- `reference/ecu_database/index.json` is still index 1.26, "updated 2026-10-21". Most of the 16 families are identify or report-only. Map hints without a citation are unverified. That is correct. The index date is stale.

### Fail-open results that look like a clean ECU

`src-tauri/src/commands_p3.rs`:

- `read_dtcs_cmd` (about line 7) turns any port error into `stored`, `pending`, and `permanent` empty arrays and `total: 0`. A disconnected adapter looks like no codes.
- `read_freeze_frame_cmd` (about line 11) turns a port error into `{}`.
- `verify_after_write` (about lines 136-146) returns `Ok("Not connected")` when there is no adapter, and `Ok("Verify note: ...")` when the live compare fails. A caller that only checks `Result` treats a failed verify as success.
- `clear_dtcs_cmd` already returns `success: false` when offline. DTC read should fail the same way.
- `read_ecu_data` in `commands_p2.rs` (about line 84) labels the offline case `honest: true`. That one is explicit. DTC read is not.

### Structure that hides unfinished work

- `lib.rs` line 2 allows `unused_imports`, `dead_code`, and `non_snake_case`. Dead commands and unused protocol helpers do not fail the build.
- The UI is a stack of versioned scripts. `index.html` loads `main.js` (~95 KB), `v312.js`, `v335.js` through `v347.js`, and `live_dashboard.js`. `v312.js` injects `v333.js` and `v334.js` at runtime. Load order is the feature system.
- `get_tuning_advice` (`commands_p3.rs`, about line 41) returns a sentence that says it does not invent a tune. It is not a tuning feature.

### Product gaps that are still true

- No cancel for long async transfers. `COMPLETION.md` v3.53.0 says so. Serial battery voltage is checked before and after a transfer, not continuously mid-write (`V2_ROADMAP.md`).
- XDF: 64-bit float and a non-zero minor stride are skipped. Strided tables and embedded axes are read-only. Non-linear math is refused. No axis editing, patches, flags, or functions.
- ELM, J2534, and STN paths exist and are not bench-tested. The P01 FullImage backup is tested against a scripted fake PCM only.
- No driver plugin SDK. BDM/JTAG is out of scope (`PRODUCT.md`). Licensed GM 5-byte keys are intentionally absent.
- CI does not run a UI test and does not sign the exe. `TuneItVerse.exe` at the repo root is gitignored. `AGENTS.md` still requires a local release copy after each ship.
- `reference/` contains flash images, WinOLS bins, `gmkeylib.dll`, kernels, and MDB files. That is a license and repo-size risk. This audit does not add, publish, or delete those files.

## Missing features that would matter in a shop

These stay inside the honesty rules: no new write path, no invented map address, no `hardware_tested: true`.

Ship next, in order:

1. Fail-closed DTC read, freeze frame, and live verify. A read that did not happen is an error.
2. Definition coverage on the open BIN: editable, read-only, skipped, and the skip reason already produced by the XDF reader. TunerPro and WinOLS do not show this.
3. A change report of working BIN versus reference (ranges, table name when a definition is loaded, old and new). Compare already lists ranges. A shop can hand this page to a customer. Export uses the existing save-text dialog.
4. Replay bench. Shipped in v3.57.0. Connect loads a checked-in ELM fixture through `replay_port`. Status says replay, not a vehicle. Flash, unlock, and clear-DTC stay refused.

Later, not in the passes that follow this audit:

- Per-cell accept of the existing STFT/LTFT preview. The preview never writes until the user applies a cell.
- Click a Live Dashboard hit cell and select that Maps cell.
- Cancel for backup, scan, and guided flash.
- One project file on top of the existing workspace import/export (stock, tune, XDF, logs, notes, capability snapshot). Do not invent a second format.
- Code-signed x64 and i686 installers. The i686 build is what loads 32-bit J2534 DLLs. Signing is what makes that build trustworthy.
- A read-only log explainer, if one is added later, that cites log rows and does not emit calibration values.

## What this pass did not do

- No cherry-pick. No merge of a historical branch. No remote branch was deleted.
- No checksum corrector, seed/key, or write path was added or enabled.

## Branch triage

Fetched every `origin` head on 2026-11-04 and tested `git merge-base --is-ancestor <tip> origin/main`.
Open pull requests: none.
Safe to cherry-pick: none. A tip that is not an ancestor of `main` is 14 to 259 commits behind. Taking it would replay an older tree, downgrade the version, or reopen a checksum or write claim that `main` later refused.
The Class column is a tag on the tip subject only. The decision on every row is the same: do not merge.
Behind = commits on `main` missing from the branch. Ahead = commits on the branch missing from `main`.

Contained in `main` (33):

- `ci/win32-i686-v354`
- `feat/aggressive-full-ops-live-pid-property-xdf-db-v2.2`
- `feat/complete-dtc-read-and-final-polish`
- `feat/complete-missing-features-v0.4.0`
- `feat/full-ops-completeness-v347`
- `feat/j2534-registry-async-io-v353`
- `feat/live-dashboard`
- `feat/log-studio-compare`
- `feat/pass1-ci-truth-pass2-dtc`
- `feat/pass3-5-honest-io-maps-flash`
- `feat/pass6-v0.3-polish`
- `feat/priority0-safety-gates-bulk-read-start`
- `feat/v1.0.1-j2534-full-surface-family-maps`
- `feat/v1.0-full-operational-industry-leading`
- `feat/v2.2.0-full-ops-unlock-flash-compat`
- `feat/v3.10.0-honest-visual-ops`
- `feat/v3.10.1-sidebar-buttons`
- `feat/v3.10.2-offline-dialogs`
- `feat/v3.12.0-wire-commands-ui`
- `feat/v3.18.1-progress-identify-cli`
- `feat/v3.26.1-identify-write-flags-ui`
- `feat/v3.30.1-uds-voltage-ui`
- `feat/v3.38-desk`
- `feat/v3.4.1-wire-catalog-ui`
- `feat/v3.5.1-live-compare-dtc-failclosed`
- `feat/v3.6.0-uds-flash-deadlock-fix`
- `feat/v3.9.1-honda-identify-correct`
- `feat/v3.9.1-ui-python-honest-log`
- `feat/xdf-float-strided-axes`
- `fix/v3.11.0-j2534-disconnect`
- `fix/v321-scripting-maplog`
- `fix/version-sync-3.49.0`
- `fix/vpw-send-crc8`

Not contained in `main` (117):

| Branch | Tip | Behind | Ahead | Class | Tip subject |
|---|---|---:|---:|---|---|
| `chore/version-sync-ui-polish-v2.7.0` | `4a1d8b0e8` | 183 | 3 | false completeness claim | docs: confirm v2.7.0 fully operational after aggressive re-analysis + version consistency polish |
| `feat/complete-operational-v345` | `113800811` | 46 | 4 | stale draft | feat: polish to v3.45.0 - operational status, version bump, docs update |
| `feat/complete-operational-v348` | `35349b05f` | 28 | 3 | stale draft | chore(release): bump tauri.conf.json to 3.48.0 |
| `feat/continuous-mid-transfer-voltage-monitoring` | `f8caf3fca` | 188 | 2 | stale draft | docs: mark continuous mid-transfer voltage monitoring as fully implemented in v2.5.1 |
| `feat/continuous-voltage-monitor-and-full-ops` | `50822f4e7` | 187 | 3 | stale draft | chore: bump version to 2.5.2 for continuous voltage monitoring completion |
| `feat/edc16c41-real-seedkey-algorithm` | `c2753fe56` | 209 | 1 | stale draft | feat(security): integrate real EDC16C41 seed/key algorithm from RE |
| `feat/edc16-multipoint-crc32` | `bc278e83e` | 258 | 2 | reopens checksum or write claim | docs(db): Update EDC16C41 to multipoint CRC32 as primary method |
| `feat/expand-ecu-db-v348` | `a3591a246` | 28 | 1 | stale draft | feat(db): add MED9_COMMON catalog family (unverified), bump to v3.48.0 |
| `feat/full-operational-enhancements-v348` | `c9d803f5f` | 28 | 5 | stale draft | docs: update COMPLETION.md for v3.48.0 marking full offline operational |
| `feat/full-ops-completeness-v3.44` | `3ae3bc74e` | 76 | 3 | stale draft | docs: update COMPLETION.md and README for v3.44.0 operational status pass |
| `feat/full-ops-v2.1-unlock-and-flash-fix` | `9eb5874f3` | 197 | 3 | false completeness claim | docs: v2.1.0 full operational status + version bump |
| `feat/full-ops-v3.44-industry-leading` | `573b672ff` | 76 | 7 | stale draft | docs: COMPLETION.md for v3.44.0 full offline operational polish |
| `feat/industry-leading-completeness-v3.48` | `d4e22b59c` | 28 | 4 | false completeness claim | docs: add v3.48.0 industry-leading completeness update |
| `feat/industry-leading-completeness-v3.50` | `83e795fd2` | 14 | 7 | stale draft | docs: update README for v3.50.0 DriverPlugin SDK |
| `feat/industry-leading-completeness-v348` | `aebb12854` | 28 | 11 | reopens checksum or write claim | docs: COMPLETION.md v3.48.0 --- experimental MEDC17 checksum support |
| `feat/industry-leading-full-ops-v3.39` | `a2ba1a2f8` | 119 | 1 | false completeness claim | feat: v3.39.0 industry-leading polish --- expanded map catalog heuristics, version sync, stronger docs. Write path st... |
| `feat/industry-leading-full-ops-v3.45` | `16c6f72da` | 46 | 7 | stale draft | feat: bump installer version to 3.45.0 |
| `feat/isotp-fc-params-and-j2534-hs` | `ef553bb2a` | 201 | 3 | stale draft | feat(flash): wire J2534 DATA_RATE into HS enter/fallback/exit |
| `feat/isotp-multiframe-bulk-read` | `fb48158ad` | 206 | 2 | stale draft | feat(flash): bulk Mode 23 read via multi-frame ISO-TP |
| `feat/j2534-passthru-ioctl` | `c34422228` | 203 | 2 | stale draft | feat(lib): register J2534 Ioctl Tauri commands (DATA_RATE, VBATT, STMIN) |
| `feat/operational-completeness-pass` | `b14601404` | 76 | 4 | stale draft | docs: note CLI strings command and completeness pass in README |
| `feat/ops-desk-v338` | `14cc018fe` | 121 | 1 | stale draft | feat(ops): add flash preflight, BIN strings, and DTC lookup |
| `feat/p01-kernel-bulk-read` | `390ef607a` | 205 | 2 | stale draft | feat(flash): kernel-assisted P01 Mode 3C bulk read |
| `feat/p01-lfsr-vs-gm2byte` | `fc38ce024` | 134 | 6 | stale draft | feat(cli): add p01cmp LFSR compare helper |
| `feat/priority0-safety-complete` | `5fc31a64e` | 207 | 2 | stale draft | docs: mark Priority 0 progress + EDC16C41 seed/key complete in V2 roadmap |
| `feat/product-software-first` | `d1d52f5c6` | 134 | 5 | stale draft | docs: park VerseLink PCB, note Transtron catalog and software-first product |
| `feat/refine-edc16c41-checksum-offsets` | `1cfb1a61c` | 259 | 2 | reopens checksum or write claim | docs(db): Update EDC16C41 checksum description to match refined region offsets and CRC32 note |
| `feat/refine-edc16c41-rust-impl` | `a6ab8f020` | 208 | 1 | stale draft | refactor(security): refine EDC16C41 Rust implementation |
| `feat/v0.5.0-full-operational-pass` | `cb71d2057` | 238 | 4 | false completeness claim | docs: update README for v0.5.0 full operational status |
| `feat/v0.6-industry-leading-polish` | `97c60b37e` | 237 | 5 | stale draft | docs: update README for v0.6.0 status |
| `feat/v0.7.0-industry-dominance-expansion` | `7e34df94a` | 236 | 6 | false completeness claim | docs: update README for v0.7.0 EDC17 expansion and full operational status |
| `feat/v0.8.0-industry-leading-full-ops` | `99802c7d6` | 235 | 7 | stale draft | feat(ui): update mocks and comments for v0.8.0 full ECU family support and operational status |
| `feat/v0.9.0-industry-leading-completion` | `59582baf0` | 234 | 4 | false completeness claim | docs: README v0.9.0 fully operational industry-leading |
| `feat/v1.0.0-full-operational-industry-leading` | `6e57ee5e7` | 233 | 3 | false completeness claim | feat(v1.0.0): wire J2534 write/read into Tauri handler + UI version bump for full operational surface |
| `feat/v1.2.0-full-industry-leading-completion` | `58f008941` | 225 | 8 | false completeness claim | feat(ui): polish main.js for v1.2.0 industry-leading status and better ECU info on connect |
| `feat/v1.3.0-full-map-coverage-and-polish` | `303c7531c` | 224 | 7 | stale draft | docs: README v1.3.0 status update |
| `feat/v1.3.0-full-operational-industry-leading` | `2be68b206` | 217 | 3 | false completeness claim | fix(ui): sync status strings and comments to v1.3.0 full operational |
| `feat/v1.4-bosch-security-j2534-full-binding-ecu-expansion` | `562f142d9` | 216 | 5 | false completeness claim | docs(db): index v1.10 for v1.4.0 full operational + Bosch path |
| `feat/v1.5.0-industry-complete` | `dd6845725` | 215 | 5 | false completeness claim | fix(ui): sync version strings to v1.5.0 and polish status for full operational |
| `feat/v1.6-complete-operational-bosch-uds-j2534-binding` | `2f19dd273` | 214 | 6 | false completeness claim | docs: README v1.6.0 status - full operational with Bosch UDS + production J2534 |
| `feat/v1.7.0-industry-polish` | `928ea29a2` | 213 | 5 | false completeness claim | docs: README v1.7.0 status --- fully operational industry-leading |
| `feat/v1.8.0-bosch-unlock-complete-and-polish` | `5933425bf` | 212 | 5 | false completeness claim | docs: README sync to v1.8.0 full operational status |
| `feat/v2.0-full-operational-completion` | `e2dbbe5ad` | 202 | 5 | false completeness claim | chore(ui): update version strings to v2.0.0 fully operational |
| `feat/v2.2-aggressive-completion-live-pid-db-auto-load-polish` | `31724507a` | 192 | 4 | false completeness claim | docs: update README for v2.2.0 aggressive analysis - confirmed fully operational industry-leading status |
| `feat/v2.3-dynamic-db-tables-live-pid-full-ops` | `0397fc3f1` | 191 | 3 | false completeness claim | docs: update COMPLETION.md for v2.3.0 --- dynamic DB-driven table auto-load + full operational status |
| `feat/v2.4-full-data-logging` | `d746deed8` | 190 | 5 | stale draft | docs: COMPLETION.md v2.4.0 --- full data logging operational |
| `feat/v2.5.3-live-pid-path-and-version-sync` | `4b35024a7` | 186 | 4 | stale draft | docs: COMPLETION.md v2.5.3 --- live Mode 01 PID path closed the last soft gap |
| `feat/v2.5-continuous-voltage-and-polish` | `bd4ec1ba7` | 189 | 4 | false completeness claim | docs: update README to v2.5.0 fully operational after aggressive analysis |
| `feat/v2.6.0-aggressive-reanalysis-polish` | `e9ca9fc71` | 185 | 1 | false completeness claim | feat: v2.6.0 --- aggressive re-analysis polish + full version sync + industry-leading confirmation |
| `feat/v2.7.0-aggressive-analysis-complete` | `54fdac4fb` | 184 | 1 | false completeness claim | feat: v2.7.0 --- aggressive full-repo analysis confirms industry-leading complete operational status |
| `feat/v2.8.0-live-log-integration-and-polish` | `0ee4768cd` | 182 | 6 | false completeness claim | docs: README v2.8.0 --- live logging feed + confirmed fully operational |
| `feat/v2.9.0-identify-512kb-cs-bin-diff-honest-flash` | `d533fc269` | 181 | 5 | stale draft | fix(ui): load v29.js helpers |
| `feat/v3.0.0-wire-v29-512kb-cs-honest-io` | `edd12a2ec` | 180 | 3 | stale draft | feat(ui): v3.0.0 labels + honest scripts tab |
| `feat/v3.1.0-wire-claimed-v3-features` | `c1402f472` | 178 | 4 | stale draft | feat: P01 512KB size constants used by checksum + flash |
| `feat/v3.11.0-protocol-aware-transport` | `7e7c168c2` | 160 | 4 | stale draft | fix(lib): restore command surface and Tauri run() after partial write |
| `feat/v3.12.0-catalog-voltage-ops` | `bf9b7ac48` | 159 | 3 | stale draft | feat(v3.12.0): UI overlay for catalog, checksum correct, flash helpers |
| `feat/v3.13.0-wire-v312-surface` | `100cb9238` | 158 | 5 | stale draft | feat(v3.13.0): catalog table, checksum/voltage buttons, load v312.js |
| `feat/v3.18.0-ops-closeout` | `098ab2b41` | 146 | 5 | stale draft | feat(v3.18.0): overlay version string |
| `feat/v3.19-bench-scripts-flash-progress` | `a19ad5afb` | 145 | 6 | stale draft | chore: overlay version 3.19.0 |
| `feat/v3.2.1-512kb-cs-failclosed-ui` | `e7291ed43` | 176 | 3 | stale draft | fix(ui): v3.2.1 labels, CSV import button, script helpers |
| `feat/v3.2.1-ui-overlay-failclosed` | `ffe3fa415` | 175 | 2 | stale draft | fix(ui): load v321.js overlay after main.js |
| `feat/v3.20.0-wire-complete` | `249ed709e` | 144 | 3 | stale draft | feat: bench script language v3.20 |
| `feat/v3.20.0-wire-scripting-flash-progress` | `ef086950c` | 144 | 4 | stale draft | feat(v3.20.0): J2534 Vbatt sag abort + voltage_warn on flash progress |
| `feat/v3.20.0-wire-scripts-flash-progress` | `3be6e852c` | 144 | 6 | stale draft | feat(v3.20.0): mark bench script language 3.20 |
| `feat/v3.20.1-wire-lib-scripts-ui` | `24431ae48` | 143 | 1 | stale draft | chore(v3.20.0): overlay status string |
| `feat/v3.21.0-wire-scripts-live-flash` | `a70100831` | 142 | 4 | stale draft | feat(v3.21.0): pub analyze_log + optional CSV map-from-log |
| `feat/v3.21-wire-scripting-flash-progress` | `616492099` | 142 | 8 | stale draft | feat: overlay injects script editor and listens for flash-progress |
| `feat/v3.22.0-ops-complete` | `0ec42f7a3` | 141 | 5 | stale draft | feat(db): load EDC15 ME9 Simos18 Trionic8 family JSON |
| `feat/v3.23.0-gm-2byte-ops` | `a3bcda9a4` | 140 | 5 | stale draft | fix(v3.23.0): pass optional algo into compute_seed_key from scripts |
| `feat/v3.24.0-p59-guard-ui-honesty` | `5b5cf07cb` | 139 | 2 | stale draft | feat(ui): catalog write column, app_info version badge, seed families |
| `feat/v3.25-complete-guards` | `6a513fe27` | 138 | 10 | stale draft | chore: overlay fallback version 3.25.0 |
| `feat/v3.25-p59-runtime-guards` | `1be6b5b93` | 137 | 2 | stale draft | fix: correct_bin_checksums_report fail-closed on P59/Honda |
| `feat/v3.26.0-identify-p59-failclosed-version-sync` | `df2cf41e8` | 136 | 3 | stale draft | feat(checksum): treat P59 like Honda --- report-only validate, fail-closed correct |
| `feat/v3.28.0-ops-complete` | `84e8a80e3` | 133 | 13 | stale draft | feat: register list_supported_adapters command |
| `feat/v3.28.1-wire-loader-ui` | `973b0100f` | 132 | 1 | stale draft | feat(db): load TRANSTRON_4HK1 family JSON |
| `feat/v3.29.0-failclosed-write-chrome` | `f96addb9e` | 131 | 5 | stale draft | feat(db): write_path_live helper used by catalog, identify, and flash |
| `feat/v3.2-real-512k-cs-failclosed` | `f406c3bd8` | 177 | 4 | stale draft | docs: only claim what this branch actually contains |
| `feat/v3.3.0-operational-closeout` | `78a5f0765` | 174 | 5 | stale draft | feat(backend): hook v33_ops into Tauri commands |
| `feat/v3.3.0-wire-missing-commands` | `b0508cd6b` | 174 | 11 | stale draft | feat(ui): load v330 overlay after v321 |
| `feat/v3.30.0-ops-complete` | `19ce420ca` | 130 | 3 | stale draft | feat(v3.30.0): capabilities matrix, pack import, versioned app_info |
| `feat/v3.30.0-uds-voltage-abort-chrome` | `ef11d6c99` | 130 | 3 | stale draft | fix(db): catalog write_allowed uses write_path_live |
| `feat/v3.31.0-operational-honesty` | `97464ab3c` | 129 | 11 | stale draft | feat(ui): catalog seed families, adapters list, 3.31 chrome fallback |
| `feat/v3.32.0-ui-complete-maplog-voltage` | `a6cc59570` | 128 | 2 | stale draft | fix(ui): version overlay 3.32.0 fallback string |
| `feat/v3.33.0-complete-ui-ops` | `eacbe3910` | 127 | 6 | stale draft | feat(ui): load v333 DOM injector from overlay |
| `feat/v3.34.0-scripts-ui-catalog-bake` | `abe425a54` | 126 | 6 | stale draft | feat(ui): load v334 script runner and sync 3.34.0 chrome |
| `feat/v3.35-supply-watch-defpack` | `135665bd8` | 125 | 1 | stale draft | feat: v3.35.0 J2534 supply watch, definition packs, self-check |
| `feat/v3.38-flash-preflight` | `ce44e3283` | 121 | 1 | stale draft | feat(flash): add offline preflight gate before guided write |
| `feat/v3.4.0-live-verify-heatmap-complete` | `1e457a4b5` | 172 | 2 | stale draft | feat: embed ME7/Delphi in ECU DB loader + v3.4.1 versions |
| `feat/v3.4.0-live-verify-heatmap-workspace` | `fa97eb7f4` | 173 | 9 | stale draft | fix: map WindowBackup into BackupResult |
| `feat/v3.48-expand-ecu-db-and-tools` | `f1ff5aaf1` | 28 | 5 | stale draft | docs: bump ECU DB index to 1.27 for v3.48.0 |
| `feat/v3.48-full-operational` | `2224dfe80` | 28 | 7 | stale draft | feat: update tauri.conf.json version to 3.48.0 |
| `feat/v3.5.0-catalog-wire-identify-ui` | `82af221ba` | 172 | 11 | stale draft | feat(ui): v3.5 overlay exposes invokeCmd for workspace export |
| `feat/v3.5.1-live-uds-and-log-pids` | `e02043c52` | 171 | 4 | stale draft | fix(ui): keep invokeCmd exposed for workspace export |
| `feat/v3.50-complete-offline-tools` | `4abca5139` | 16 | 1 | stale draft | feat(scripts): add range stats command and bump to v3.50.0 |
| `feat/v3.50-polish-and-completeness` | `79195eb94` | 16 | 7 | stale draft | feat: sync version to 3.50.0 in frontend HTML |
| `feat/v3.6.0-uds-flash-and-deadlock-fix` | `cbcedebfd` | 170 | 3 | stale draft | fix: read_properties mutex deadlock + fail-closed L1/L2 unlock + v3.6.0 UI |
| `feat/v3.7.0-xdf-a2l-table-tools` | `a53db869b` | 169 | 7 | stale draft | feat(ui): wire v3.7.0 A2L and table-math controls |
| `feat/v3.7.1-ops-closeout` | `03206b15c` | 168 | 5 | stale draft | feat(cli): identify 1.5MB SID803 dumps in bench helper |
| `feat/v3.8.0-honest-live-honda-guard-workspace` | `49b40eaf5` | 167 | 5 | stale draft | feat(ui): inject UDS/Honda controls and strip offline-demo live numbers |
| `feat/v3.9.0-wire-cs-guard-honest-pids` | `e1955ffba` | 166 | 4 | stale draft | chore(ui): v380 overlay version 3.9.0 |
| `feat/v3.9.1-honda-cs-workspace-uds-flash` | `d1ff26de9` | 165 | 4 | reopens checksum or write claim | feat(flash): Honda refuse + Bosch UDS 0x34/36 download path |
| `feat/v327-identify-write-flags` | `700f64f5a` | 135 | 4 | stale draft | fix: restore compare/workspace/maplog after identify split |
| `feat/v336-session-tools` | `2169ff976` | 124 | 1 | stale draft | feat(session): fingerprint scores, log stats, and tune projects that cannot enable write |
| `feat/v339-ops-desk` | `dbf7438ce` | 119 | 1 | stale draft | feat(ops): add bench planner, driver plugin SDK, and coverage desk |
| `feat/v339-readiness-wiring` | `736096ec9` | 119 | 1 | stale draft | feat: v3.39.0 wire missing desks and add readiness |
| `feat/v348-complete-features` | `3b7a0c16f` | 28 | 5 | stale draft | docs: update README for v3.48.0 status |
| `feat/v360-osid-j2534-hexpoke` | `c4b3e5138` | 170 | 10 | stale draft | feat: wire v3.6.0 OS-ID live PIDs, hex poke, J2534 registry list |
| `feat/vpw-high-speed-mode` | `2c05788c3` | 204 | 3 | stale draft | fix(flash): correct format! in Mode 0xA1 soft-accept log |
| `feature/uds-full-layer` | `34bd5a6e0` | 199 | 4 | stale draft | feat(can): complete raw Lawicel multi-frame ISO-TP TX path |
| `fix/ci-version-3.49.0` | `7cb44c02c` | 16 | 1 | stale draft | fix: align Cargo.lock, index.html and main.js versions to 3.49.0 (CI) |
| `fix/complete-lib-rs-and-full-operational` | `80d348cbc` | 194 | 2 | stale draft | fix(core): correct dtc call sites (read_dtcs / clear_dtcs with prior), remove non-existent pid_decode/vpw calls, use ... |
| `fix/complete-unlock-commands-and-guided-flash-compat` | `4a81bd6c0` | 195 | 2 | stale draft | feat(security): expose unlock_level1, unlock_level2, bosch_uds_unlock as Tauri commands for full end-to-end unlock su... |
| `fix/restore-full-lib-rs-build` | `596b68863` | 259 | 1 | stale draft | fix: restore complete lib.rs (was accidentally truncated) + initialize fs/dialog/opener plugins. This resolves all un... |
| `refactor/isotp-flow-control` | `956ffc275` | 200 | 1 | stale draft | refactor(can): typed ISO-TP flow control, shared assembler, unified retry |
| `test/serial-integration` | `b89dd4131` | 197 | 5 | stale draft | temp: remove broken lib.rs before restore |
| `test/serial-integration-v2` | `90f1297f8` | 197 | 3 | stale draft | test: wire MockSerialPort + serial integration tests into can module |
| `test/uds-unit-coverage` | `65172c6b2` | 198 | 1 | stale draft | test(uds): expand unit coverage for builders, NRC, ALFI, transfer, sessions |
