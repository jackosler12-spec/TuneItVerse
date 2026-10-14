# TuneItVerse ECU Database

Machine-readable ECU family metadata. Embedded at compile time by `src-tauri/src/ecu_database.rs`.

## How to add a family
1. Create `reference/ecu_database/<family>.json` with **cited public sources**.
2. Mark each capability `verified` / `unverified` / `report_only` / `experimental_unverified`.
3. Add `include_str!` + push in `load_ecu_database()`.
4. Update `index.json` and this README.
5. Rebuild. Personal dumps only — no commercial bins.

## Live write path
Only `P01_0411` is in `write_families()`. Guided write still requires FullImage backup, voltage gate, OS/chip match, and (for the kernel path) `experimental_p01_write` + typed confirmation. **Not bench-tested.** EDC16C41 has measured seed/key but checksum is report-only and write is off.

## Honesty
Do not claim "fully operational" for a family without measured checksums, a logged or cited flash session, and a FullImage backup path. Catalog entries without those are identify / offline-tools only.
