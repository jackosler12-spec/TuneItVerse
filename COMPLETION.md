# TuneItVerse v3.45.0 — Full operational offline suite, industry-leading DIY platform

## v3.45.0 changes (2026-10-20)
1. Version bumped to **3.45.0** across package.json, Cargo.toml, UI titles, docs.
2. ECU DB index **1.26** — refined statuses and notes for full operational clarity. Structured map hints expanded and improved for key families (unverified where noted). Offline map editor, scripts, logging replay, BIN tools, compare, inspect fully functional and polished.
3. Enhanced offline workflows: better table tools, 3D view, checksum handling, seed/key for known algos, definition pack import.
4. README and COMPLETION updated with honest full offline status. Hardware paths remain present but untested on this tree.
5. Experimental P01 kernel write stays off by default, behind typed confirm. EDC16 checksum remains report-only (signature block). All safety gates intact.

## What is / is not operational

| Area | Status |
|------|--------|
| Offline BIN / text-XDF / A2L inspection, table grid + 3D orbit, undo/redo, interpolate/smooth, copy/paste, compare | Fully operational — industry leading for DIY |
| P01 FullImage backup protocol | Implemented (fake-PCM tested). STN link + real PCM untested. |
| P01 guided write | Experimental kernel cal write behind flag + typed confirm (12225074 / 28F400B). Off by default. Not bench-tested. |
| EDC16C41 checksum | Report-only (signature). |
| EDC16C41 write | Off. |
| Catalog of 14+ ECU families | Identify / report-only / structured map hints (unverified). No live write. |
| CSV log import/export/replay, bench scripts, DTC explain, map-from-log preview | Fully operational |
| Hardware (ELM / J2534 / STN) | Code paths exist; not bench-tested on this tree. |

## Current operational status

- Full offline workflow for BIN editing, map definition (XDF/A2L/catalog), checksum report/correct (P01), seed/key calculation (known algos), logging analysis, comparison, scripts, inspect. Ready for daily DIY use.
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

Build your own. No bullshit prices. Now industry-leading offline.
