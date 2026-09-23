# TuneItVerse Product Direction

**Updated:** 2026-09-23  
**Status:** Software-first. VerseLink Apex PCB work is parked.

## Decision

JRTuners will not design or capture VerseLink Apex PCBs in-house. KiCad is out of scope. The product that ships now is **TuneItVerse** — the desktop platform — talking to hardware you already own.

VerseLink Apex remains a future hardware option only if a third party (freelancer / design house) builds it from the existing design package. It is not a blocker for product development.

## What ships today

TuneItVerse v3.27+ on Windows:

- Identify / checksum / patch personal BIN dumps
- Guided flash with fail-closed voltage gate, honest backup quality, live verify
- Write path advertised only for **P01_0411** and **EDC16C41**
- Live data + DTCs via ELM ASCII (CAN/UDS) and raw VPW when selected
- J2534 when a vendor DLL is present
- Offline seed/key for GM 2-byte tables and dump-derived `seed_tables.json` pairs

## Official adapters (now)

See `reference/adapters/supported_adapters.json`.

| Adapter | Role |
|---|---|
| OBDLink MX+ | Primary daily driver (Bluetooth + Car Scanner / serial path) |
| ELM327-class USB/BT | Diagnostics + limited protocol work |
| J2534 pass-thru | Windows programming path when DLL is installed |
| FTDI USB-serial | Bench / legacy serial |
| VerseLink Apex | Parked. Not required to use the product. |

## Write-enabled platforms

| Family | Vehicle focus | Write |
|---|---|---|
| P01_0411 | Holden / GM LS1 0411 PCM | Live path |
| EDC16C41 | Nissan Patrol GU ZD30CRD | Live path |
| GM_P59 | Truck/SUV PCM | Identify only until your measured checksum words exist |
| TRANSTRON_4HK1 | Isuzu FRR 4HK1 | Catalog only until a personal dump + protocol notes land |

Everything else in the catalog is identify / map-hint only. No fake write flags.

## Next product work (no PCB)

1. Bench the two write platforms on your vehicles with OBDLink MX+ / J2534.
2. Drop measured seed pairs into `reference/ecu_database/seed_tables.json`.
3. Promote Transtron 4HK1 from catalog to protocol once you have a dump and confirm UDS/KWP behaviour.
4. Keep adding families as JSON + loader include — no core rewrite.
5. Surface adapter list in the UI so the product is honest about what it talks to.

## Out of scope until hardware is outsourced

- KiCad / Gerbers / enclosure
- BDM / JTAG as a first-class live path
- Custom USB firmware for VerseLink

Build your own software. Buy or reuse adapters. No bullshit prices.
