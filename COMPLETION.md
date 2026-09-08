# TuneItVerse v3.11.0 — protocol-aware live I/O (2026-09-08)

v3.10.2 made offline BIN/XDF work. Live data on a cheap ELM327 still looked dead because `read_ecu_data` / Mode 09 / DTC always sent raw J1850 VPW frames. J2534 connect never flipped connection health, so the UI stayed Disconnected after a successful PassThru open.

## What this pass actually changed

1. New `transport.rs`: ELM ASCII Mode 01 / Mode 09 (`010C`, `0902`) when the session is CAN / UDS / ELM / auto. Raw VPW frames only when the protocol is VPW/J1850.
2. `get_connection_health` reports `Connected (j2534)` when a PassThru device is open. `j2534_connect` stamps `STATE.protocol`. `disconnect_ecu` releases the J2534 handle.
3. Connect warmup calls `elm_init_can_500k`, `consult_init`, or `kwp_fast_init` based on the radio the user picked.
4. DTC read uses ELM services 03 / 07 / 0A on ASCII transports instead of VPW headers.
5. Battery voltage prefers J2534 `READ_VBATT`, then ELM PID 0x42, then VPW.
6. UDS download re-checks J2534 voltage every 4 KB when a PassThru device is open.
7. Dashboard lists the embedded ECU catalog (`list_ecu_catalog`).
8. Versions 3.11.0 across package, crate, Tauri window, HTML, workspace export.

## Still needs your bench

1. EDC17 / MED17 / ME7 / SID803 / Honda seed tables from **your** dumps. Starter algebra is not a measured key and will not unlock.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 / SID block checksum routines measured on a personal dump before a corrector ships.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
