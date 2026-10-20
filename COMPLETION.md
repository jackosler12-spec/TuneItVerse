# TuneItVerse v3.45.0 — ELM/OBDLink live-data fixes from a real Patrol log

## v3.45.0 changes (2026-10-20)
Source: Jack's Car Scanner + OBDLink MX+ log from his 2007 Nissan Patrol GU ZD30CRD (CAN 11-bit 500k), via the Data Logger agent. Only redacted adapter frames are committed (`reference/fixtures/elm_patrol_zd30_obdlink.txt`).

1. **ELM reply parsing fixed.** With `ATH1` the old parser joined all hex and cut pairs, so the `7E8` header shifted every byte and real replies (`7E804410C0BBC`, `7E80441230CF6`, `7E803410550`) decoded to nothing. `elm_parse.rs` now parses per line: strips 11-bit/29-bit CAN or 3-byte J1850/KWP headers, follows the ISO-TP PCI (single, first, consecutive frames), keeps waiting on `7F xx 78`, reports other `7F` NRCs, `NO DATA` and adapter errors (`CAN ERROR`, `STOPPED`, `BUFFER FULL`, `?`) as errors. Text like `SEARCHING...` no longer leaks hex letters.
2. **Live data deadlock fixed.** `transport::current_protocol()` re-locked the global STATE mutex while `with_port` already held it, so every live read (dashboard, logging, read_properties) would hang. The protocol now lives in the transport session cache.
3. **Supported-PID polling.** Reads and caches 0100/0120/0140... per session and polls only listed PIDs. Added SAE PID 0x23 fuel rail gauge pressure ((A*256+B)*10 kPa) and 0x21. The Diesel OBD template now includes rail (it is a standard PID).
4. **Timing.** 1000 ms read window (adapter `ATST96` = 600 ms), 6 s for the first request (protocol search), response-count suffix (`010C1`) once CAN is confirmed, read no longer ends on a serial timeout before the prompt, STPTO 600 instead of 100. Logging stops after 10 consecutive NO DATA with "ignition off?".
5. **Adapter state.** `elm_init_can_500k` no longer sends a second `ATZ` (it wiped the OBDLink setup). Mode 01 restores `ATCAF1 ATH1 ATS0 ATST96` after ISO-TP code switched to `ATCAF0`.
6. **Voltage.** Adapter STVR/ATRV first; PID 0x42 only when the ECU lists it (the Patrol does not). The UI/audit label is the real source (`adapter_stvr`, `adapter_atrv`, `pid42`, `pid42_vpw_raw`); it used to say `pid42` for adapter readings. Raw VPW frames are no longer written to ELM ports as a fallback.
7. **Nissan CAN diagnostic path (read-only).** `ATSH7E0`, `10C0`→`50C0`, `2211xx/2212xx/2213xx` support masks, optional raw `22xxxx` reads, always `1081` + `ATSH7DF`. The 32 IDs seen on the Patrol are listed as "supported, meaning unknown". No names or scaling assigned. Old K-line Consult code is marked UNVERIFIED.
8. **CSV import.** Car Scanner long format (`SECONDS;PID;VALUE;UNITS;LATITUDE;LONGTITUDE`) is pivoted per timestamp; GPS columns are dropped in all imports.
9. **Replay transport.** `replay_port.rs` replays the fixture as a serial port so CI covers warmup/text-mode setup, support masks, Mode 01 polling, Mode 09 (cal ID, blank VIN), ATRV and the Nissan scan. J1850 VPW frames from the same log verify CRC-8; `vpw::validate_frame` now accepts the real J1850 CRC-8 on received frames.
10. **Fingerprint.** The Patrol reports cal ID `0VS43B`, an all-zero VIN and `090A` NO DATA. Nothing confirms EDC16C41; the DB marks the vehicle mapping **unconfirmed** and `read_properties` shows EDC16C41 only as `candidate_family`.
11. Connect screen: Adapter Status / Bus Monitor buttons now exist and their script is loaded (it was not), plus Supported PIDs and Nissan CAN Scan.

**Unverified in v3.45.0:** none of this has run against a live car from this build; it is tested on replayed real frames only. ISO-TP flow control on multi-frame replies is left to the adapter (ATCAF1). Nissan data-ID meanings are unknown. Raw-VPW TX still appends an additive byte, not the J1850 CRC (ELM/STN adapters add the CRC themselves). The `vpw` protocol option sets up an OBDLink with STP 12 but then sends raw binary frames; that path needs a separate look.

## v3.44.0 (previous)

1. Version bumped to **3.44.0** across package.json, Cargo.toml, tauri.conf.json, installer, UI titles.
2. ECU DB index **1.25** — statuses refined for operational clarity. Structured map hints (addr / size / scale / unit) expanded for EDC16C41, P59, and Transtron 4HK1 (all marked unverified). Offline map editor, scripts, logging replay, BIN tools fully functional.
3. Refined map address hints in catalog entries where community typical (ME7, EDC16, Delphi etc.). Still mark unverified; personal dumps required for precision.
4. README and COMPLETION updated with honest full offline status. Hardware paths remain untested.
5. Experimental P01 kernel write stays off by default, behind typed confirm. EDC16 checksum remains report-only (signature block).

## What is / is not operational

| Area | Status |
|------|--------|
| Offline BIN / text-XDF / A2L inspection, table grid + 3D, undo/redo, interpolate, compare | Fully operational |
| P01 FullImage backup protocol | Implemented (fake-PCM tested). STN link + real PCM untested. |
| P01 guided write | Experimental kernel cal write behind flag + typed confirm (12225074 / 28F400B). Off by default. Not bench-tested. |
| EDC16C41 checksum | Report-only (signature). |
| EDC16C41 write | Off. |
| Catalog of 14+ ECU families | Identify / report-only / structured map hints (unverified). No live write. |
| CSV log import/export/replay, bench scripts, DTC explain | Fully operational |
| ELM/OBDLink Mode 01 live data + logging | Parser, support masks, timing fixed; tested on replayed real Patrol frames. Not run on a live car from this build. |
| Nissan CAN read-only scan | Implemented from observed frames; ID meanings unknown. |
| Hardware (ELM / J2534 / STN) | Code paths exist; not bench-tested on this tree. |

## Current operational status

- Full offline workflow for BIN editing, map definition (XDF/A2L/catalog), checksum report (P01 correctable), seed/key calculation (known algos), logging analysis, comparison, scripts.
- Guided flash gates stay fail-closed. After a verified FullImage backup, P01 write stays off unless `experimental_p01_write` is set and the typed confirm phrase matches. No other family has a write path. Experimental write is not bench-tested.
- Safety: voltage gates, audit log, fail-closed on unknown family / bad checksum / no backup.

Never flash without a verified backup and stable power. Personal dumps only.

## Still needs your bench

1. Measured P59 checksum words and kernel. Writes stay blocked.
2. EDC17 / MED17 / ME7 / SID803 / Honda / Simos / T8 / Transtron seed tables from **your** dumps. Put pairs in `seed_tables.json`.
3. PCM Hammer comparison on your 512 KB P01 OS (`12225074`).
4. A vendor J2534 DLL on Windows so mid-write Vbatt and the registry walk are live.
5. Bench-test the experimental P01 kernel cal write on your STN + PCM (protocol from public PcmHammer host source; still hardware-unverified). FullImage backup already exists.
6. Licensed GM 5-byte keys. We do not ship that library.
7. Isuzu FRR Transtron dump + protocol notes before any 4HK1 write work.

Build your own. No bullshit prices.
