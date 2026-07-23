# TuneItVerse v0.3.0 — Honest Status (2026-07-23)

**Status: v0.3.0 DIY platform** — offline edit + best-effort live diagnostics for P01 / EDC16.  
Not a full commercial replacement. Prefer fail-closed over fake success.

## Done in this engagement (merged to main)

| Pass | Deliverable | PR |
|------|-------------|-----|
| 1 | CI (`cargo check` + `cargo test` + npm), honest docs, close PR #32 | #36 |
| 2 | DTC read/freeze/clear + Diagnostics UI | #36 |
| 3 | Honest I/O, pid_decode live data, j2534 registered, compare/verify UI | #37 |
| 4 | P01 auto tables from real `reference/16263425.xml` | #37 |
| 5 | Flash Mode 34→36→37, image CRC (no `0xDEADBEEF`) | #37 |
| 6 | v0.3.0 bump, OOB patch refuse, recovery modal, scripts honesty | this |

## What works

- Multi-view desktop UI (Tauri 2)
- Serial connect + multi-protocol init (VPW / CAN / KWP / Consult)
- DTC diagnostics tab (Modes 03/07/0A/04/02)
- Live Mode 01 PIDs via `pid_decode` (errors if disconnected)
- P01 table catalogue from real XML addresses
- Checksum validate/correct (P01 + EDC16 multipoint CRC32 best-effort)
- Guided flash scaffolding with Mode 34/36/37 + recovery prompt UI
- Unit tests: 38 passing (`cargo test --lib`)
- CI on main via `.github/workflows/ci.yml`

## Still missing (honest)

1. **J2534** — surface registered; no full Windows `libloading` + registry enum yet  
2. **Kernel full-PCM backup** — current backup is partial Mode 22 sampling  
3. **Live post-flash readback inside guided pipeline** — use Verify button  
4. **EDC16 maps** — community start addresses; verify with WinOLS before write  
5. **Embedded Python** — templates only; `python/ecu_scripting.py` is external  
6. **More ECUs** — only P01 / EDC16 / P59-meta  
7. **Hardware validation** — not in CI  
8. **UDS full flash / robust multi-frame ISO-TP**  

## Build & test

```bash
npm install
cd src-tauri
cargo check
cargo test --lib
cd ..
npm run build    # Windows release (Tauri)
```

## Safety

Never flash without a verified backup, stable power, and correct map definitions.  
Wrong table addresses can brick an ECU — OOB patches are refused by the backend.

## License

MIT — see `LICENSE`.
