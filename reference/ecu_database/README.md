# TuneItVerse ECU Database

Machine-readable backend database for known ECUs. Powers identification on OBD connect, checksum correction before flashing, security access, and map/.xdf loading in the Tauri app.

## Structure
- One .json per ECU family
- master_index.json (future)
- Used by Rust backend (src-tauri) via Tauri commands e.g. `get_ecu_by_os_id`, `correct_checksum_for_ecu`

## Current Entries
- p01_0411.json : Holden LS1 / GM P01 0411 PCM (already referenced with bin, XDF, checksum plugin, kernel)
- edc16c41_nissan_patrol.json : Bosch EDC16C41 for Nissan Patrol GU (23710-VS43B / ZD30 variants)

## Workflow
See tuneitverse skill instructions for full bin dump -> checksum correct -> XDF/maps -> DB integration process.

All work is for personal ECU ownership and legal tuning only. No copyrighted files distributed here.

Next: Implement loader + checksum routines in Rust. Add more ECUs as needed.