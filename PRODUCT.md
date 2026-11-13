# TuneItVerse Product Direction

**Updated:** 2026-11-05 (v3.61.0: value sheet of extracted cell changes. Unread addresses are not compared. Still not bench-tested on a vehicle. Installers unsigned unless a certificate is configured.)  
**Status:** Software-first. VerseLink Apex PCB work is parked.

## Decision

JRTuners will not design or capture VerseLink Apex PCBs in-house. KiCad is out of scope. The product that ships now is **TuneItVerse** — the desktop platform — talking to hardware you already own.

VerseLink Apex remains a future hardware option only if a third party builds it from the existing design package. It is not a blocker.

## What ships today

TuneItVerse v3.61.0 on Windows:

- Value sheet cites extracted stock and tuned cells for loaded tables. An address that does not resolve is skipped. `write_allowed` stays false
- Trim report cites logged trim percent and does not emit a calibration value. Three-way compare classifies stock, the working tune, and a third dump. `write_allowed` stays false
- Per-cell STFT accept, dashboard hit selects the open map cell, cancel for backup / scan / flash, one workspace JSON project file, and a log explainer that cites rows
- x64 and i686 installer script. The exe and installer are unsigned unless `SIGN_RELEASES` and a certificate are configured. No certificate ships in the repo
- Replay fixture on Connect: a checked-in ELM trace, status says replay, write and clear stay refused
- Identify / checksum / patch personal BIN dumps
- Guided flash with fail-closed voltage gate, honest backup quality, live verify
- The only write path is an **experimental** P01_0411 kernel calibration write. It is off by default, behind a typed confirm, and not bench-tested.
- Live data + DTCs via ELM ASCII (CAN/UDS) and raw VPW when selected
- J2534 when a vendor DLL is present
- Offline seed/key for GM 2-byte tables and dump-derived `seed_tables.json` pairs
- Map-from-log occupancy. The mean RPM/MAP bin is `mean_cell`. It does not emit a VE value or a calibration factor
- Official adapter list in the Connect page

## Official adapters (now)

See `reference/adapters/supported_adapters.json`.

| Adapter | Role |
|---|---|
| OBDLink MX+ | Primary daily driver |
| ELM327-class USB/BT | Diagnostics + limited protocol work |
| J2534 pass-thru | Windows programming path when DLL is installed |
| FTDI USB-serial | Bench / legacy serial |
| VerseLink Apex | Parked. Not required. |

## Write-enabled platforms

| Family | Vehicle focus | Write |
|---|---|---|
| P01_0411 | Holden / GM LS1 0411 PCM | Experimental (off by default, not bench-tested) |
| EDC16C41 | Nissan Patrol GU ZD30CRD | Off (checksum report-only; signature block) |
| GM_P59 / GM_P10 / GM_P12 | GM PCMs | Identify only (PcmHammer PcmInfo.cs) |
| TRANSTRON_4HK1 | Isuzu FRR 4HK1 | Catalog only until a personal dump + protocol notes land |

Everything else in the catalog is identify / map-hint only. No fake write flags. The in-app Capabilities page lists every family's per-capability status and source.

## Out of scope until hardware is outsourced

- KiCad / Gerbers / enclosure
- BDM / JTAG as a first-class live path
- Custom USB firmware for VerseLink
- Licensed GM 5-byte keys

Build your own software. Buy or reuse adapters. No bullshit prices.
