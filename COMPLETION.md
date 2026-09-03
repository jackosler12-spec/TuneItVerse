# TuneItVerse v3.6.0 — OS-ID fingerprint + J2534 registry + hex poke (2026-09-03)

**Status: v3.5.1 closed the live ISO-TP verify lie. This pass closes identify-by-size-only and the fake J2534 device list.**

## What this pass actually changed

1. `identify_bin` scans printable strings and matches catalog OS / part tokens (e.g. `12225074`) instead of trusting BIN size alone. Size collisions stay listed.
2. `j2534_list_devices` walks `HKLM\\SOFTWARE\\PassThruSupport.04.04` (and WOW6432Node / 04.00) on Windows via `reg query`. Non-Windows stays honest.
3. `patch_bin_bytes_cmd` pokes bytes into a loaded image. UI hex poke uses it.
4. Checksum **validation** on unknown sizes is report-only. **Correction** stays fail-closed. Still no invented ME7 corrector.
5. Live log / Mode 01 path pulls O2 B1S1/B1S2, OBD baro (PID 0x33 = kPa), fuel status, fuel level.
6. Versions synced to 3.6.0.

## Still needs your bench

1. EDC17 / MED17 / ME7 seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows so the registry walk returns a real FunctionLibrary path.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 block checksum routine measured on a personal dump before a corrector ships.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
