# TuneItVerse — Honest Status (2026-07-23)

**Status: v0.2.x — usable DIY scaffold for P01 / EDC16 offline edit + best-effort live diagnostics. Not a full commercial-tool replacement.**

## What works

| Area | Reality |
|------|---------|
| Tauri multi-view UI | Dashboard, Connect, Live, Diagnostics, Tables, Flash, Scripts |
| Serial connect | Real open + protocol init; port list is honest (errors if enumeration fails) |
| Live data | Mode 01 + `pid_decode`; **errors when disconnected** (no mock gauges) |
| Checksum | P01 additive + EDC16 multipoint CRC32 (best-effort regions) |
| Security seed-key P01 | L1/L2 algorithms + unlock path |
| DTC | Read / freeze / clear **wired + Diagnostics UI** |
| Tables | **P01 auto-load from real `reference/16263425.xml` addresses**; EDC16 community start addresses (documented as verify-before-write) |
| Flash | Guided pipeline uses **Mode 34 → 36 → 37**; image CRC recorded (no fake `0xDEADBEEF`); live verify command fails closed if disconnected |
| J2534 | Module **compiled & registered**; needs Windows + vendor DLL for live use |
| Compare / verify UI | Flash tab buttons for compare-bin-to-ECU and verify-after-write |
| CI | `ci.yml`: cargo check + cargo test --lib + npm sanity |

## Still missing / limited

1. Full J2534 DLL load (`libloading`) + registry device enum on Windows  
2. Kernel-based full PCM backup (current backup is partial Mode 22 sampling)  
3. Live post-flash readback inside guided pipeline (caller can use `verify_after_write`)  
4. EDC16 map addresses are community starting points, not WinOLS-locked  
5. Python scripting integration  
6. More ECU families beyond P01 / EDC16 / P59-meta  
7. Hardware-in-the-loop validation (not possible in CI)  
8. Full UDS flash / multi-frame ISO-TP reliability layer  

## Pass log

- **Pass 1:** CI + honest COMPLETION + version align; closed PR #32  
- **Pass 2:** DTC commands + Diagnostics UI  
- **Pass 3:** Honest I/O (no fake ports/live/verify success); wire pid_decode; register j2534; compare/verify UI  
- **Pass 4:** P01 tables from `16263425.xml` real addresses  
- **Pass 5:** Flash Mode 34/36/37 + real image CRC; write paths use RequestDownload  

## Build

```bash
npm install
cd src-tauri && cargo check && cargo test --lib
cd .. && npm run build
```

## License

MIT — see `LICENSE`.
