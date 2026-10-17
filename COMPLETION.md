# TuneItVerse v3.44.0 — Operational status panel + honesty matrix

## v3.44.0 changes (2026-10-17)
1. Added dashboard Operational Status panel (v344.js) that surfaces an honest capability matrix via get_operational_status (command landing in follow-up).
2. Version synced to 3.44.0 across package.json, Cargo.toml, tauri.conf.json, installer, UI title and banners.
3. ECU DB index noted for next update.
4. All prior features remain: offline BIN/XDF/A2L editor fully operational, P01 CS1+CVN + full backup protocol (fake-PCM tested), experimental kernel write behind flag + typed confirm (off by default, not bench-tested), EDC16C41 identify + measured seed/key + report-only checksum, catalog of 14 families (most identify/report-only).

## What is fully operational offline
- BIN load, identify, table grid + 3D, undo/redo, interpolate, compare, save.
- Text XDF / A2L load.
- P01 checksum correction (CS1 + CVN).
- GM 2-byte and EDC16C41 4-byte seed/key offline compute.
- Bench scripts, self-check, CSV log import/replay/trim preview.
- Definition packs (hints only).

## What still needs work for "full industry-leading"
- Hardware paths (ELM327, J2534, STN) implemented in code but **not bench-tested** on this tree.
- EDC16C41 checksum correction: report-only. Public multipoint layout not found; signature block at 0x1FDF78 is proprietary.
- Other ECU families (EDC17, MED17, ME7, SID803, Honda, Simos, T8, Transtron): catalog + starter maps only. Real seed/key and checksum require **your personal dumps**.
- P01 experimental kernel write: protocol from public PcmHammer spec, not bench-tested.
- Full async I/O, BDM/JTAG, plugin SDK still future.

Never flash without a verified backup and stable power. Personal dumps only. Build your own. No bullshit prices.

---
Previous v3.43.0 content follows for history.
