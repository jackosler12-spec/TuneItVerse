# TuneItVerse v3.61.0 — ECU DB index 1.27 and version polish

## v3.61.0 changes

- ECU database index bumped to 1.27 with updated notes on catalog families and public source citations.
- Version strings synced to 3.61.0 across package.json, Cargo.toml, and tauri.conf.json.
- Offline analysis and documentation refined. No new write paths, no checksum correctors enabled beyond P01, hardware_tested remains false.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

# TuneItVerse v3.60.0 — value sheet

## v3.60.0 changes

- Compare has Value sheet. It lists extracted cell changes for every loaded table: row, column, stock, tuned, and delta. Identical tables are one summary line. An address that does not resolve is `not compared` and no number is invented for it.
- Cell delta now returns an error for that same unread address instead of comparing a zero placeholder grid.
- A read-only table whose cells still extract is compared, and the sheet quotes the read-only note. Comparing it does not enable patch. `write_allowed` stays false.
- Export report saves the sheet with the existing text dialog. Nothing is patched.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

(Previous content continues as before; full historical log preserved in prior commits.)
