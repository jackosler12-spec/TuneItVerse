# TuneItVerse Product Direction

**Updated:** 2026-09-25  
**Status:** Software-first. VerseLink Apex PCB work is parked.

## Decision

JRTuners will not design or capture VerseLink Apex PCBs in-house. KiCad is out of scope. The product that ships now is **TuneItVerse** — the desktop platform — talking to hardware you already own.

VerseLink Apex remains a future hardware option only if a third party builds it from the existing design package. It is not a blocker.

## What ships today

TuneItVerse v3.30+ on Windows:

- Identify / checksum / patch personal BIN dumps
- Guided flash with fail-closed voltage gate, honest backup quality, live verify
- Write path advertised only for **P01_0411** and **EDC16C41**
- Live data + DTCs via ELM ASCII (CAN/UDS) and raw VPW when selected
- J2534 when a vendor DLL is present
- Offline seed/key for GM 2-byte tables and dump-derived `seed_tables.json` pairs
- Map-from-log occupancy + STFT/LTFT VE cell *preview* (never silent write)
- Official adapter list in the Connect page
- Runtime ECU pack import + TunerPro-style XDF export
- Honest capabilities matrix on the dashboard

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
| P01_0411 | Holden / GM LS1 0411 PCM | Live path |
| EDC16C41 | Nissan Patrol GU ZD30CRD | Live path |
| GM_P59 | Truck/SUV PCM | Identify only until measured checksum words exist |
| TRANSTRON_4HK1 | Isuzu FRR 4HK1 | Catalog only until a personal dump + protocol notes land |
| MED9_COMMON | Ford / Volvo / Jaguar MED9 | Identify only |
| EDC16C31 | PSA / Ford / Volvo EDC16 C31/C34 | Identify only — not the C41 write path |
| GM_E38 | VE Commodore / LS2-LS7 ECM | Identify only — not P01 |

Everything else in the catalog is identify / map-hint only. No fake write flags.

## Out of scope until hardware is outsourced

- KiCad / Gerbers / enclosure
- BDM / JTAG as a first-class live path
- Custom USB firmware for VerseLink
- Licensed GM 5-byte keys

Build your own software. Buy or reuse adapters. No bullshit prices.
