# TuneItVerse — Honest Status (2026-07-23)

**Status: PARTIAL — usable scaffold for DIY P01 / EDC16 offline edit + best-effort live diagnostics. Not a complete industry replacement for commercial tools.**

Previous COMPLETION.md claims of “fully operational / production-ready / no critical gaps” were **overstated**. This file tracks reality.

## What works (with caveats)

| Area | Reality |
|------|---------|
| Tauri shell + multi-view UI | Yes — Dashboard, Connect, Live, Diagnostics, Tables, Flash, Scripts |
| Serial connect (ELM/VPW/CAN/KWP/Consult init) | Best-effort open + protocol init; needs real adapter |
| Checksum validate/correct | Implemented for P01 additive + EDC16 multipoint CRC32 (best-effort offsets) |
| Security seed-key (P01 L1/L2) | Algorithm ported; needs hardware to prove |
| DTC read / freeze / clear | **Wired end-to-end (Pass 2)** — backend + Diagnostics tab |
| XDF/table extract & patch | Parser + extract/patch path present |
| Guided flash UI + orchestration | Scaffold with real serial frames; **verify/safety incomplete** (Pass 5 target) |
| ECU DB | P01, EDC16C41, GM_P59 metadata embedded |

## Critical gaps remaining

1. **Table auto-load addresses are placeholders** for many maps — risk of wrong-offset patches until Pass 4.
2. **J2534** — module exists but is a stub (not fully bound to vendor DLLs).
3. **Flash pipeline** — Mode 34/erase/verify incomplete; placeholder verify CRC historically.
4. **pid_decode.rs** largely unused by live data path.
5. **Mock/fake data** on disconnect or failed reads (being removed in Pass 3).
6. **Python scripting** — stub only.
7. **ECU breadth** — only three families; P59 metadata-only.
8. **Hardware validation** — not performed in CI; requires vehicle/bench.

## Pass log

### Pass 1 — CI & truth (this branch)
- Replaced broken Node-only CI with `ci.yml`: `cargo check`, `cargo test --lib`, npm sanity.
- Disabled npm-publish (desktop app, not a package).
- Aligned `Cargo.toml` version to `0.2.0`.
- Closed obsolete PR #32.
- Rewrote this status file honestly.

### Pass 2 — DTC end-to-end
- Registered `read_dtcs_cmd`, `read_freeze_frame_cmd`, `clear_dtcs_cmd`.
- Added Diagnostics / DTC view in UI (read / freeze / clear).

### Planned
- Pass 3: Honest I/O, wire pid_decode, register j2534, compare UI
- Pass 4: Real P01 map addresses from reference XML
- Pass 5: Flash Mode 34/36/37 + real verify
- Pass 6: README / release polish → v0.3.0

## Build

```bash
npm install
cd src-tauri && cargo check && cargo test --lib
cd .. && npm run build   # full Tauri release (Windows)
```

## License

MIT — see `LICENSE`.
