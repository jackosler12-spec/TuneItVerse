# TuneItVerse v3.61.0 — bulk UDS Mode 23 coverage

## v3.61.0 changes

- Live backup / verify for Bosch UDS families now attempts multi-chunk Mode 23 reads (256-byte chunks, up to 32) for better coverage of calibration areas.
- Notes report collected bytes and approximate percentage of expected image size.
- Still classified as PartialDidOnly. FullImage requires verified complete dump. Write gates remain fail-closed.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

# TuneItVerse v3.60.0 — value sheet

## v3.60.0 changes

- Compare has Value sheet. It lists extracted cell changes for every loaded table: row, column, stock, tuned, and delta. Identical tables are one summary line. An address that does not resolve is `not compared` and no number is invented for it.
- Cell delta now returns an error for that same unread address instead of comparing a zero placeholder grid.
- A read-only table whose cells still extract is compared, and the sheet quotes the read-only note. Comparing it does not enable patch. `write_allowed` stays false.
- Export report saves the sheet with the existing text dialog. Nothing is patched.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

# TuneItVerse v3.59.0 — honest logs and three-way compare

## v3.59.0 changes

- Trim report keeps hits and the logged trim percent. It no longer emits a multiplier or tells the user to apply one.
- Map-from-log names the mean RPM/MAP occupancy bin `mean_cell`. The old `suggested_ve_cell` key is gone. No VE number is added.
- Guided flash and live compare return the original link error. A disconnect stays `Not connected. Call connect_ecu first.` A gate failure is not prefixed with `Not connected:`.
- Compare has Three-way. It classifies bytes as tune only, other only, or both differ from stock, and names a table only when the loaded definition already covers that range. Export still saves the last report as text. Nothing is patched.
- `V2_ROADMAP.md` no longer marks a full EDC16/EDC17/MED17 ISO-TP dump as done. That path is still a windowed `PartialDidOnly` probe. This release does not add the dump.
- **Not hardware-tested.** No adapter and no vehicle were used. Installers stay unsigned unless a certificate is configured.

Previous versions retained for history. See full log in prior sections if needed.

Offline BIN/XDF/A2L/logging/scripts remain fully operational. Build your own.
