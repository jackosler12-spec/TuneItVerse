# TuneItVerse v3.46.0 — Nissan DID reads, adapter cleanup, response-pending handling

## v3.46.0 changes (2026-10-20)
Source: further redacted frames from the same Car Scanner/OBDLink log (Data Logger agent). Committed fixtures: `reference/fixtures/elm_patrol_nissan_22reads.txt` (18 KB slice of the 2.6 MB read dump, ECU 7E8 frames only) and `reference/fixtures/elm_cleanup_and_pending.txt` (13 KB). The full dump is not committed.

1. **Support masks.** Tested: the 2211xx/2212xx/2213xx masks decode to exactly the 30 DIDs Car Scanner polled plus 1143 and 120B (MSB of the first byte = base+1). No code change was needed.
2. **DID value reads.** DID lengths as observed: 1101–1147 one byte, 1207–130E two bytes. 1143/120B were never read, so their length is unknown and they are read one at a time. Reads go 3 DIDs per request; every DID echo and length is checked. Any multi-read failure (`7F 22 23`, `7F 22 21`, NO DATA, mismatch) falls back to single reads. A single `7F 22 21` is retried once. Optional log channels `nissan_XXXX` ("Nissan DID XXXX (unknown meaning, raw)", off by default) log the raw big-endian value. The EDC16C41 DB entry has `nissan_did_observations` (length, min/max raw, distinct values, read counts, meaning "unknown"). F190/F18C are probed and reported as "not supported (NRC ..)". Their bytes are never stored. Replay strips Car Scanner's frame-count suffix (`221101110211042` → `22110111021104`).
3. **Adapter cleanup.** A Drop guard restores adapter state on every exit, error or not. The ELM ISO-TP path restores `ATCAF1 ATH1 ATSH7DF`. The Nissan scan and poll restore `1081 ATSH7DF`. Replies that carry CAN frames from non-OBD IDs (the 11 Aug `2D1`/`23D` flood after `STCAF0` + `ATCM000`) trigger a re-init (`ATCAF1 STCAF1 ATCRA ATAR ATFCSM0 STFAC ATSH7DF ATH1 ATS0 ATST96`). A valid 7E8 answer in the same reply is used; otherwise there is one retry. Foreign frames are never decoded. `NO DATA` on the ISO-TP path is an error (it used to hex-decode to `0xDA`).
4. **7F xx 78 after the prompt.** After a response-pending, the app keeps listening for up to P2* (5 s) without sending anything, even after the `>`. If an ECU is still pending when that window ends, its answer to the next request with the same SID is reported as ambiguous and discarded. Answers are matched per ECU by SID and PID/DID echo, so a stray `59 02 FF` is never taken as the answer to another request. Tests use the real multi-ECU `19 02 xx` transcripts and the Patrol `0902` case.

**Unverified in v3.46.0:** replay-tested only, nothing has run on a live car from this build. DID meanings and scaling are unknown. The late-answer-after-prompt case is a synthetic test, because the log has no example of it. Two lengths are unknown: 1143 and 120B. Not every adapter supports `STCAF1`/`STFAC`/`ATFCSM0` (an unknown command answers `?` and is ignored). The F190/F18C NRCs come from the Data Logger's report and are not in the committed fixtures.

## v3.45.0 — ELM/OBDLink live-data fixes from a real Patrol log

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
| Nissan CAN read-only scan + raw DID logging | Implemented from observed frames (3-DID reads, single fallback); ID meanings unknown. |
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
