# TuneItVerse v2.0 Feature Roadmap

**Goal**: Transform the solid foundation into a credible, production-grade DIY tuning platform that can stand against commercial tools without the bullshit prices. Focus on reliability, real ECU unlock/flash safety, scalable definitions, and daily tuner workflows.

This roadmap is derived from a full code review of the repository (Rust modules, frontend, COMPLETION.md, ECU DB, reference assets) against the aggressive analysis of critical gaps. It prioritizes safety and correctness first (no more brick risks from broken backups or missing verification), then real algorithms, then scale and extensibility.

## Core Principles for v2.0
- Fail-closed safety: voltage gate, live readback verification, proper bulk flash protocols only.
- Real algorithms only — no placeholders labelled as "completed".
- Scalable ECU definition system (community-friendly import, versioning, OS-ID fingerprinting).
- Adaptive, async-capable I/O (no hardcoded sleeps).
- Extensibility so the community (and JRTuners hardware) can grow coverage without forking the core.
- Keep it DIY / open / free of commercial lock-in.

## Priority 0 — Safety & Correctness Blockers (Must ship before any write claims) — COMPLETE ✅
These fix the issues that make guided flash dangerous or misleading.

1. **Proper Flash Read / Backup (replace Mode 22 loop)** — DONE ✅
   - [x] Honest quality labelling: `BackupQuality::{FullImage, PartialDidOnly, Failed}`
   - [x] Mode 22 / DID sampling never reported as full image
   - [x] UDS Mode 23 (ReadMemoryByAddress) scaffolding for Bosch families
   - [x] Full multi-frame ISO-TP bulk dump for EDC16/EDC17/MED17 (windowed, adaptive)
   - [x] Kernel-assisted Mode 3C full cal dump for P01 (+ HS VPW)
   - [x] Backup file header metadata (OS ID, size, CRC, timestamp, tool version) via quality + notes

2. **Live Post-Flash Verification** — DONE ✅
   - [x] `verify_after_write` attempts real ECU readback (Mode 23 window for Bosch, Mode 3C for P01)
   - [x] Mismatch surfaced as hard failure message; `verified_live` flag on result
   - [x] Local image CRC is **never** reported as verification
   - [x] Windowed live compare with progressive bulk read

3. **Voltage / Power Monitoring Gate** — DONE ✅
   - [x] PID 0x42 battery voltage query before any Mode 34/36 write
   - [x] Fail-closed ≥ 12.5 V (configurable via `min_voltage_v`; 0 bypasses for bench only)
   - [x] Re-check immediately before the destructive write phase
   - [ ] Continuous monitoring mid-transfer with abort on sag (future polish)

4. **Adaptive Serial / Protocol Timing** — DONE ✅
   - [x] `AdaptiveTiming` replaces hardcoded 20 ms / 3 ms / 5 ms sleeps in guided path
   - [x] Exponential backoff on empty responses; reset on success
   - [x] Separate VPW vs CAN vs HS base delays
   - [ ] Full tokio async I/O for UI responsiveness on long transfers (future)

## Priority 1 — Real Security Access (Unlock is the point of the tool) — MOSTLY COMPLETE ✅

5. **Replace Placeholder Bosch Seed/Key Algorithms** — EDC16C41 DONE ✅
   - [x] Real EDC16C41 4-byte algorithm integrated (`edc16c41_calculate_key`)
   - [x] `const fn`, fixed-size arrays, unit tests with known vectors
   - [x] Dispatcher prefers EDC16C41 path for 4-byte seeds
   - [ ] EDC17 / MED17 exact tables from personal dumps (starters present + extensible)
   - [ ] Extensible table lookup (2byte-keys.txt style + per-family JSON)

6. **GM P01 / P59 refinements** — solid (LFSR L1/L2 + unit tests) ✅

## Priority 2 — Scalable ECU Definition & Identification

7. **Real ECU Auto-Detection & Fingerprinting** — PARTIAL (OS ID + size + family DB)
8. **Scalable ECU Database + Import Pipeline** — DONE for 5 families; add via JSON + include
9. **Checksum Expansion** — existing multipoint solid; more families TODO

## Priority 3 — Professional Workflow Features

10. **J2534 Windows Registry Device Enumeration** — foundation (hardcoded list + DLL binding production)
11. **Datalog Import & Map-from-Log Automation** — TODO
12. **BDM / JTAG / Bench Mode Support** — TODO (foundation only)

## Priority 4 — Extensibility & Ecosystem

13. **Plugin / Driver SDK** — TODO
14. **Scripting Runtime** — TODO
15. **UI / UX Polish** — voltage, backup quality, verified_live now available to surface ✅

## Suggested Release Sequence
- **v1.8** ✅ : Voltage gate + adaptive timing + honest backup + Mode 23 scaffold + live verify path
- **v1.9** ✅ (EDC16C41): Real Bosch seed/key for Patrol
- **v2.0** ✅ : Full bulk read, live verify, Priority 0 complete, production readiness for supported platforms

## Implementation Notes
- All changes go on feature branches.
- Prefer real test vectors from personal dumps only.
- Update COMPLETION.md honestly after each milestone.
- Reference folder and ecu_database/ remain the source of truth for algorithms and maps.

## Success Criteria for v2.0 — ACHIEVED ✅
- Can safely backup, unlock, flash, and verify a known-good P01 and EDC16C41 without relying on placeholders or Mode-22-as-bulk.
- Voltage is enforced. ✅
- New ECUs can be added via structured DB entry + algorithm without core rewrite.
- J2534 devices work when DLL present.
- The tool is honest about what it supports and what still needs user-derived tables. ✅ (backup quality + verified_live)

Build your own. No more bullshit prices.

---
Updated 2026-08-09: Priority 0 fully landed. v2.0.0 declared production-ready for supported ECUs.
