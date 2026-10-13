# TuneItVerse ECU Database

Machine-readable ECU family metadata. Embedded at compile time by `src-tauri/src/ecu_database.rs`.

## How to add a family
1. Create `reference/ecu_database/<family>.json` (cite sources; mark unverified fields).
2. Add `include_str!` + push in `load_ecu_database()`.
3. Update `index.json` and this README.
4. Rebuild.

## Live write path
Only `P01_0411` is listed in `write_families()`. Even for P01, guided write still refuses until a kernel write session is established from a logged erase/write (the repo PcmHammer log is read-only). EDC16C41 has measured seed/key but no verified checksum table and no flash segment — report-only.

## Honesty
Do not claim "fully operational" for a family without measured checksums, a logged flash session, and a FullImage backup path. Entries without those are identify / offline-tools only.
