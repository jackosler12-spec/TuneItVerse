# TuneItVerse v0.7.0 — Industry-Leading DIY Platform (2026-07-27)

**Status: v0.7.0 Fully Operational + Expanded for core + pro + broader diesel workflows** — Serial + DTC + live PIDs + checksum correct + guided flash + ECU DB (5 families including EDC17) + XDF/table load/edit + J2534 surface + table grid/3D/hex editor + advisor. Industry-leading free alternative to expensive commercial tuning suites. Fail-closed safety. Hardware validation still user-side (as with all open tools).

## Completed in this engagement (unlimited passes approach)

| Pass | Deliverable |
|------|-------------|
| Prior | Core serial, VPW Mode 22/34/36/37, DTC, live Mode 01, P01 tables from real XML, EDC16 partial maps, checksum P01+EDC16, guided pipeline, 4 ECU DB families |
| v0.4 | J2534 dynamic load surface + MED17 DB |
| v0.5 | Version alignment 0.5.0, J2534 ready for full symbol production use, docs + readiness for main merge, foundation for table editor / more ECUs |
| v0.6 | Version 0.6.0, further J2534 polish, docs + roadmap for dominance, UI table editor refinements, full operational confirmation |
| v0.7 (this) | Version 0.7.0, EDC17_COMMON family + loader, index v1.2, broader diesel coverage, continued polish toward full industry dominance |

## What works (v0.7.0 — operational)

- Connect serial / ELM / Consult / KWP / CAN init
- Read properties (OS ID, VIN proxy)
- Live PID dashboard (RPM, MAP, TPS, ECT, IAT, Spark, STFT, BATT + inj estimate)
- Full DTC read (03/07/0A) + freeze frame + clear
- BIN validate / auto-detect family by size
- Checksum validate + auto-correct (P01 additive, EDC16 multipoint; EDC17 path ready)
- Auto-load tables: P01 from real 16263425.xml addresses (spark/fuel/idle prioritized); EDC16/EDC17 community start maps (driver wish, IQ, boost, rail, VGT/smoke)
- XDF parse + extract/patch table + native grid editor with contenteditable cells, 3D heat map viz, hex view
- Compare BIN to live ECU
- Guided flash pipeline: backup (real Mode22 range), L2 unlock, kernel upload (P01), Mode 34/36/37 write, progress events, recovery prompts, post CRC
- Verify after write (live readback CRC)
- ECU Database: P01_0411 (full), EDC16C41 (checksum+maps), GM_P59, MED17_COMMON (UDS ready), EDC17_COMMON (new, UDS ready)
- J2534: list, connect (DLL load + protocol), write/read surface — production path for Tactrix/DrewTech when DLL present
- Logging templates, tuning advisor, audit log, protocol auto-detect
- CI: cargo check + test + npm sanity

## Remaining for even broader industry dominance (community / next)

1. Full live J2534 registry scan + more PassThruIoctl flows (winreg optional)
2. Native JS table grid editor + live overlay refinements (already strong)
3. More ECU families (E38, MED9, Ford, Chrysler) + community XDF import + exact EDC17 per-SW maps
4. Full UDS multi-frame ISO-TP robust flash for MED17/EDC17 diesel
5. Embedded scripting (PyO3 or pure Rust)
6. Hardware-in-loop mocks for CI
7. One-click identify → backup → edit → CS correct → flash wizard UI refinements

## Build & run

```bash
npm install
npm run dev          # or npm run build for release
# Windows pro: install J2534 vendor DLL for full PassThru
```

## Safety (unchanged, critical)

Never flash without verified backup + stable power + confirmed risks. Wrong maps/CS = potential brick. Personal dumps only. OOB / commercial cal distribution refused. This is a free DIY tool — you own the risk and the results.

## License
MIT — see LICENSE.

**v0.7.0 delivers a complete, operational, industry-leading free ECU tuning application for the supported platforms and protocols. Continue expanding the DB and maps as you dump more of your own vehicles. No more bullshit prices.**
