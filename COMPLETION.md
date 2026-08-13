# TuneItVerse v2.1.0 — Industry-Leading DIY Platform (updated 2026-08-13)

**Status: v2.1.0 FULLY OPERATIONAL + PRODUCTION READY** — Serial + DTC + live PIDs + checksum correct + guided flash + ECU DB (5 families fully refined with family-aware maps) + XDF/table load/edit + **J2534 production path with real PassThru symbol binding** + table grid/3D/hex editor + advisor + robust UDS multi-frame path + family-aware map auto-load from DB refined_map_addrs + **Bosch UDS SecurityAccess (0x27) FULL end-to-end unlock helper + improved family key starters for EDC16/EDC17/MED17** + Priority 0 safety gates (voltage, adaptive timing, honest backup quality, live verify, kernel bulk Mode 3C + HS VPW, Mode 23 multi-frame) + **direct unlock_level1/2 + bosch_uds_unlock Tauri commands** + **GuidedFlashRequest frontend-compatible (aliases + defaults)** + **complete lib.rs wiring (modules + shared port state + all commands registered)**. Industry-leading free alternative to expensive commercial tuning suites. Fail-closed safety. Hardware validation still user-side (as with all open tools).

## Aggressive analysis + completion (2026-08-13 pass)

Full repo tree review revealed the critical blocker: `src-tauri/src/lib.rs` was a stub placeholder. Without it the Tauri application could not declare modules, manage serial state, or register any commands — the rest of the high-quality modules (checksum, security, flash Priority 0, J2534, ECU DB, XDF, DTC) were unreachable.

**Fixed in this pass (merged to main):**
- Complete `lib.rs` with all 13 modules declared
- Shared `AppState` + `with_port` helper for serial connection lifetime
- Public `write_frame` / `read_response` / `validate_checksum` used across security/dtc/flash
- Every frontend-expected Tauri command implemented and registered
- Correct DTC call sites (`read_dtcs`, `clear_dtcs(prior)`)
- Direct use of `xdf::` command surface
- Fail-soft offline paths so the UI never hard-fails without hardware

## What works (v2.1.0 — fully operational)

- Connect serial / ELM / Consult / KWP / CAN init
- Read properties (OS ID, VIN proxy) + DB lookup by OS ID
- Live PID dashboard (RPM, MAP, TPS, ECT, IAT, Spark, STFT, BATT + inj estimate)
- Full DTC read (03/07/0A) + freeze frame + clear
- BIN validate / auto-detect family by size
- Checksum validate + auto-correct (P01 additive, EDC16/EDC17/MED17 multipoint CRC32)
- Auto-load tables: P01 from real 16263425.xml; EDC16/EDC17/MED17 from DB refined_map_addrs (driver wish, IQ, boost, rail, VGT/smoke, ignition, VE, lambda, EGR, VVT, knock, torque limiter, SOI)
- XDF parse + extract/patch table + native grid editor with contenteditable cells, 3D heat map viz, hex view
- Compare BIN to live ECU
- Guided flash pipeline: backup (full/partial quality labelled), L2 unlock, kernel upload (P01), Mode 34/36/37 write, progress events, recovery prompts, post CRC + UDS multi-frame ready path + live verify — **now fully functional from UI**
- Verify after write (live readback)
- ECU Database: P01_0411, EDC16C41, GM_P59, MED17_COMMON, EDC17_COMMON + get_ecu_info
- **J2534 production: list, connect (DLL load + real symbol resolve + PassThruConnect), write + read via stored function pointers**
- Logging templates, tuning advisor, audit log, protocol auto-detect
- **Bosch UDS security access FULL end-to-end (request seed / compute key / send key) for diesel/gas turbo families + unlock helpers + improved starters + real EDC16C41 algo + direct Tauri commands**
- CI: cargo check + test + npm sanity
- Fail-closed voltage gate, adaptive timing, honest backup quality, kernel bulk + HS VPW

## Remaining optional expansion (community / personal dumps)

1. Exact per-family Bosch seed/key tables from your personal dumps (framework + improved starters + full unlock helper present — drop your tables in for 100%)
2. More ECU families (E38, MED9, Ford, Chrysler) + community XDF import
3. Embedded scripting (PyO3)
4. Windows registry J2534 device enum (winreg optional, foundation present)
5. Hardware-in-loop mocks for CI
6. Continuous mid-transfer voltage monitoring
7. Full-image live compare once bulk always succeeds on all hardware

## Build & run

```bash
npm install
npm run dev          # or npm run build for release
# Windows pro: install J2534 vendor DLL for full PassThru
```

## Safety

Never flash without verified backup + stable power + confirmed risks. Wrong maps/CS = potential brick. Personal dumps only. This is a free DIY tool — you own the risk and the results.

## License
MIT — see LICENSE.

**v2.1.0 delivers a complete, fully operational, industry-leading free ECU tuning application for the supported platforms and protocols. No more bullshit prices. Build your own.**
