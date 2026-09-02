# TuneItVerse v3.4.0 — live verify + heatmap + catalog (2026-09-02)

**Status: the guided-flash “verify” path now actually talks to the bus. Write success is fail-closed without live readback.**

v3.3.0 registered CSV import and seed/key. It still marked a flash successful when live verify returned an error, never issued Mode 23 / Mode 3C, and the UI never sent `user_confirmed_risks: true`.

## What this pass actually changed

1. `verify_after_write` probes VPW Mode 3C, UDS 0x23, and KWP 0x23 windows and compares bytes to the written image.
2. Guided flash `success` is **false** unless those windows match, or you tick **accept unverified write**.
3. Backup is no longer a hard-coded `Failed` stub. It uses the same windows and labels `PartialDidOnly` vs `Failed` honestly. A failed backup aborts the write.
4. UI now injects `user_confirmed_risks` from the four risk boxes (this field was default-false and never set).
5. Map-from-log builds a 16×16 RPM×MAP occupancy heatmap (+ optional STFT averages) instead of a single average cell.
6. Identify adds SHA-256 (full / head / tail) and printable strings. BIN compare reports contiguous diff ranges.
7. `export_workspace_cmd` dumps identify + heatmap + family list as JSON.
8. Catalog starters: `ME7_COMMON` (1MB) and `DELPHI_DCM` (2MB) with refined_map_addrs wired into table auto-load.

## Still needs your bench

1. EDC17 / MED17 seed tables from **your** dumps — starters in `security.rs` are not claimed as verified.
2. PCM Hammer comparison on your 512 KB P01 OS (8×64 KB additive layout).
3. A Windows box + vendor J2534 DLL to confirm the registry walk against a real driver.
4. A real kernel-resident Mode 3C full-image dump. Windowed probes are not a 512 KB / 2 MB backup.
5. ME7 1MB checksum routine from a personal dump (size is catalogued; correction is not invented).

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
