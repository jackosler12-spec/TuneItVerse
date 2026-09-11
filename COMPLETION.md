# TuneItVerse v3.15.0 — P01 TableSeek pack (~1598 Universal Patcher parameters)

Loading a Holden P01 BIN used to show three stub maps. The real pack is `reference/tableseek-p01-p59.xml` (1598 TableSeek entries). v3.15.0 embeds that pack and locates tables with the same SearchStr rules as Universal Patcher: `*` is a wildcard, `@` bytes are the address, `+D12` follows a 32-bit pointer. Hits only — no invented addresses. Any 128/512 KB dump that is not Honda gets the pack (Holden OS IDs are not only `12225074`). On the reference LS1 `12225074` dump this locates 1300+ parameters; tables whose opcode sequence is absent from that OS are listed as missing. Honda 512 KB dumps are not given the P01 pack. Tables UI: search + category chips.

---

# TuneItVerse v3.14.0 — live flash progress + P59/P01 corrector guard (2026-09-11)

v3.13.0 wired catalog/checksum/voltage UI. This pass closes three operational holes that were still live on main:

1. Guided flash discarded the progress callback (` |_| {} `), so the flash bar never moved during a write.
2. A 512 KB image with a P59 OS (`12586243` / `12602801`) was treated as P01-additive-safe. Those CS words are not the P01 map. Correction and write now fail-closed for P59-only strings.
3. Identify result was not published on `window.lastIdentify`, so the flash overlay could not pick the family the Tables view just resolved.

## What this pass actually changed

1. Version 3.14.0 across package, crate, Tauri window, installer, HTML, overlay, workspace export, and docs.
2. `guided_flash_pipeline` takes `AppHandle` and emits `flash-progress` `{percent, bytes_done, bytes_total}` on every VPW chunk and UDS download tick. UI listens and updates the bar live.
3. `cs_guard::looks_like_gm_p59` + `p59_blocks_p01_corrector`. Validate is report-only. Correct and guided write refuse.
4. Identify sets `gm_p59_os`, `correction_safe` only for measured P01 / EDC16 paths, and `window.lastIdentify`.
5. BIN compare side panel lists diff ranges. CLI `python3 python/ecu_scripting.py diff a.bin b.bin`.

## Still needs your bench

1. Measured P59 checksum words and kernel. Do not invent them.
2. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
5. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
