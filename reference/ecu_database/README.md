# TuneItVerse ECU Database

Machine-readable backend database for known ECUs. Powers identification on OBD connect, checksum correction before flashing, security access, and map/.xdf loading in the Tauri app.

## Structure
- One .json per ECU family
- index.json (master index)
- Used by Rust backend (src-tauri/src/ecu_database.rs) via Tauri commands e.g. `get_ecu_by_os_id`, `get_ecu_info`, `auto_load_tables_for_bin`, `correct_checksum_for_ecu`

## Current Entries (v1.3.0 fully operational)
- p01_0411.json : Holden LS1 / GM P01 0411 PCM (full — checksum, security L1/L2, XDF, kernels, recovery paths)
- edc16c41_nissan_patrol.json : Bosch EDC16C41 for Nissan Patrol GU (23710-VS43B / ZD30) — multipoint CRC32 + refined maps (driver wish, IQ, boost, rail, VGT, smoke, EGR, torque limiter, SOI)
- gm_p59.json : GM P59 PCM (truck/SUV) — full metadata + checksum path
- med17_common.json : Bosch MED17 (VW/Audi gasoline turbo common) — full protocol + community maps + checksum ready
- edc17_common.json : Bosch EDC17 (common diesel platforms) — full operational UDS flash path, multipoint CS, refined start maps

## Workflow
See tuneitverse skill instructions for full bin dump -> checksum correct -> XDF/maps -> DB integration process.

All work is for personal ECU ownership and legal tuning only. No copyrighted files distributed here.

## How the loader works
`ecu_database.rs` embeds the JSON at compile time via `include_str!`. Add a new family by:
1. Create `reference/ecu_database/your_family.json` following the schema
2. Add `const YOUR_JSON: &str = include_str!(...);` + push in `load_ecu_database()`
3. Update `index.json` + this README
4. Rebuild — frontend gets it via `list_supported_ecus` / `get_ecu_info` / auto-load

**v1.3.0 status:** Complete operational industry-leading free platform. Expand with your own verified dumps. No more bullshit prices.
