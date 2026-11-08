# TuneItVerse v3.61.0 — expanded bulk UDS probe

## v3.61.0 changes

- New `bulk_read.rs` module: multi-window UDS Mode 23 collection up to 32KB for Bosch families (EDC/MED/ME7 etc.). Uses existing `read_memory_by_address` and `live_address_plan`. Returns PartialDidOnly quality with CRC. Not a full dump. Not hardware-tested.
- Module registered in lib.rs. Version bumped across package.json, Cargo.toml, tauri.conf.json.
- Offline tools (BIN/XDF/A2L edit, compare, logging, scripts, dashboard) remain fully operational.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.
- `write_allowed` stays false. No new write paths.

# TuneItVerse v3.60.0 — value sheet

## v3.60.0 changes

- Compare has Value sheet. It lists extracted cell changes for every loaded table: row, column, stock, tuned, and delta. Identical tables are one summary line. An address that does not resolve is `not compared` and no number is invented for it.
- Cell delta now returns an error for that same unread address instead of comparing a zero placeholder grid.
- A read-only table whose cells still extract is compared, and the sheet quotes the read-only note. Comparing it does not enable patch. `write_allowed` stays false.
- Export report saves the sheet with the existing text dialog. Nothing is patched.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

# (previous sections retained for history)
