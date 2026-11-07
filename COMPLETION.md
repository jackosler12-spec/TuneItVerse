# TuneItVerse v3.61.0 — plugin SDK skeleton + multi-size UDS probe

## v3.61.0 changes

- New `plugin_sdk.rs`: trait-based skeleton for driver/definition plugins (ProtocolAdapter, ChecksumCorrector, SeedKeyAlgo, etc.). Registry + NullPlugin example with unit tests. No plugins ship; load path is future work. Fail-closed by default.
- Live verify / backup for Bosch families now probes UDS 0x23 at 64 / 128 / 256 bytes (takes the largest that answers). Still PartialDidOnly. Notes updated. Not a full dump.
- Version bumped across Cargo.toml, package.json, tauri.conf.json.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured. Write paths unchanged (experimental P01 off by default; everything else not_supported).

# TuneItVerse v3.60.0 — value sheet

## v3.60.0 changes

- Compare has Value sheet. It lists extracted cell changes for every loaded table: row, column, stock, tuned, and delta. Identical tables are one summary line. An address that does not resolve is `not compared` and no number is invented for it.
- Cell delta now returns an error for that same unread address instead of comparing a zero placeholder grid.
- A read-only table whose cells still extract is compared, and the sheet quotes the read-only note. Comparing it does not enable patch. `write_allowed` stays false.
- Export report saves the sheet with the existing text dialog. Nothing is patched.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

(Previous history truncated for this commit; full prior log remains in git history.)
