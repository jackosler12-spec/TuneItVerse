# TuneItVerse v2.0 Feature Roadmap

**Goal**: Transform the solid v1.7.0 foundation into a credible, production-grade DIY tuning platform that can stand against commercial tools without the bullshit prices. Focus on reliability, real ECU unlock/flash safety, scalable definitions, and daily tuner workflows.

This roadmap is derived from a full code review of the repository (Rust modules, frontend, COMPLETION.md, ECU DB, reference assets) against the aggressive analysis of critical gaps. It prioritizes safety and correctness first (no more brick risks from broken backups or missing verification), then real algorithms, then scale and extensibility.

## Core Principles for v2.0
- Fail-closed safety: voltage gate, live readback verification, proper bulk flash protocols only.
- Real algorithms only — no placeholders labelled as "completed".
- Scalable ECU definition system (community-friendly import, versioning, OS-ID fingerprinting).
- Adaptive, async-capable I/O (no hardcoded sleeps).
- Extensibility so the community (and JRTuners hardware) can grow coverage without forking the core.
- Keep it DIY / open / free of commercial lock-in.

## Priority 0 — Safety & Correctness Blockers (Must ship before any write claims)
These fix the issues that make current guided flash dangerous or misleading.

1. **Proper Flash Read / Backup (replace Mode 22 loop)**
   - Implement real bulk memory read for supported families:
     - P01 / GM VPW: Kernel-assisted Mode 23 / ALDL fast upload or Mode 3D style after kernel in RAM.
     - Bosch EDC16/EDC17/MED17: Proper UDS ReadMemoryByAddress (0x23) with multi-frame ISO-TP, or family-specific dump routines.
   - Fallback path that clearly marks "partial / DID-only" vs "full flash image".
   - Produce verifiable .bin backups with header metadata (OS ID, size, CRC, timestamp, tool version).

2. **Live Post-Flash Verification**
   - Implement `verify_after_write` that actually reads back from the ECU (using the new bulk read) and compares CRC / selected regions / full image against the written data.
   - Surface mismatch as hard failure + recovery prompt.
   - Do not report the CRC of the local image as "verification".

3. **Voltage / Power Monitoring Gate**
   - Before any Mode 34/36/erase/write: query battery voltage (OBD PID 0x42 or equivalent) and enforce ≥ 12.5 V (configurable, with hard abort).
   - Continuous monitoring during long transfers; abort + recovery if voltage sags.
   - UI checkbox alone is not safety — this is the real fail-closed gate.

4. **Adaptive Serial / Protocol Timing**
   - Remove hardcoded `thread::sleep(20ms)` etc.
   - Response-length-aware polling, adaptive timeouts based on protocol (VPW vs ISO-TP vs KWP), retry with backoff.
   - Move flash I/O toward async (tokio) where practical so UI stays responsive and timing is precise.

## Priority 1 — Real Security Access (Unlock is the point of the tool)

5. **Replace Placeholder Bosch Seed/Key Algorithms**
   - `bosch_key_from_seed` currently uses invented XOR/rotate patterns explicitly marked as starting points.
   - Derive and implement real tables / algorithms from personal dumps (EDC16C41 392203 / Patrol, EDC17, MED17).
   - Support common community patterns + extensible table lookup (reference/2byte-keys.txt style + per-family JSON or binary tables).
   - Full end-to-end UDS 0x27 flow with level selection, attempt counting, and clear NRC handling.
   - Unit tests against known seed→key pairs from dumps.

6. **GM P01 / P59 refinements**
   - Already strong (LFSR L1/L2). Add any missing levels or known variants; keep unit tests.

## Priority 2 — Scalable ECU Definition & Identification

7. **Real ECU Auto-Detection & Fingerprinting**
   - Live handshake: query VIN / OS ID / hardware ID / software version via protocol-specific DIDs or Mode 22 / UDS 0x22.
   - Map to verified DB entry; reject unknown units before allowing writes.
   - Replace simple `contains()` string match with proper matching (exact OS, fuzzy family, version ranges).

8. **Scalable ECU Database + Import Pipeline**
   - Expand beyond the current 5 families (P01_0411, EDC16C41, GM_P59, MED17_COMMON, EDC17_COMMON).
   - Schema evolution: versioned entries, A2L / DAMOS / XDF community import, map address validation.
   - Master index + per-family JSON (or SQLite for larger scale).
   - Auto-load corresponding XDF / refined_map_addrs on bin open or connect.
   - Community contribution path (PRs with test bins + checksum routines + seed tables).

9. **Checksum Expansion**
   - Keep existing P01 additive + EDC16/17/MED17 multipoint CRC32 solid.
   - Add more family-specific correctors as DB grows (MED9, E38, etc.).
   - Always validate + correct before write; surface report in UI.

## Priority 3 — Professional Workflow Features

10. **J2534 Windows Registry Device Enumeration**
    - Use winreg (or equivalent) to discover installed J2534 devices from the Windows Registry instead of requiring manual DLL path.
    - List name, vendor, DLL path; auto-select or let user pick.
    - Keep current symbol binding + PassThru* calls.

11. **Datalog Import & Map-from-Log Automation**
    - Import CSV / MegaLogViewer / EFILive-style logs.
    - Basic VE table / fuel / timing correction suggestions from WOT pulls, lambda, knock counts.
    - Closed-loop correction workflow (simple first version).
    - Export corrected maps back to XDF / bin.

12. **BDM / JTAG / Bench Mode Support (foundation)**
    - Even if full hardware is in VerseLink Apex, provide software stubs + protocol scaffolding for BDM100-style / JTAG recovery of locked or virgin ECUs.
    - Kernel / loader management for recovery scenarios.
    - Safety warnings for bench power requirements.

## Priority 4 — Extensibility & Ecosystem

13. **Plugin / Driver SDK**
    - Define a clean interface for external drivers, definition packs, and importers (Rust trait or dynamic load, or PyO3 scripting runtime).
    - Allow community to add ECU families, seed tables, checksum routines, and log parsers without core changes.
    - Document the SDK early so adoption can start.

14. **Scripting Runtime (PyO3 or similar)**
    - Optional Python scripting surface for power users (map math, batch processing, custom PIDs).

15. **UI / UX Polish for Daily Use**
    - Guided flash must surface voltage, backup status, verification result, and recovery steps clearly.
    - Progress + cancel for long operations.
    - Better error messages with actionable recovery (not just "failed").

## Suggested Release Sequence
- **v1.8** (hotfixes): Voltage gate + adaptive timing + start of real bulk read for P01.
- **v1.9**: Live verify_after_write + real Bosch seed/key for EDC16C41 (user's Patrol).
- **v2.0**: Full Priority 0–2 + J2534 enum + initial datalog import + expanded DB (at least 8–10 families) + SDK skeleton.

## Implementation Notes
- All changes go on feature branches (e.g. `feat/flash-bulk-read`, `feat/bosch-real-seedkey`, `feat/ecu-db-v2`).
- Prefer real test vectors from personal dumps only.
- Update COMPLETION.md honestly after each milestone — no more "fully operational / industry-leading" claims until the blockers are closed.
- Reference folder and ecu_database/ remain the source of truth for algorithms and maps.

## Success Criteria for v2.0
- Can safely backup, unlock, flash, and verify a known-good P01 and EDC16C41 without relying on placeholders or Mode-22-as-bulk.
- Voltage is enforced.
- New ECUs can be added via structured DB entry + algorithm without core rewrite.
- J2534 devices appear automatically on Windows.
- A tuner can import a log and get a useful map correction suggestion.
- The tool is honest about what it supports and what still needs user-derived tables.

Build your own. No more bullshit prices.

---
Generated from full repository review + gap analysis. Track progress by closing items and opening PRs against this roadmap.
