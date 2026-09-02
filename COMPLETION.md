# TuneItVerse v3.6.0 — UDS flash path + deadlock fix (2026-09-02)

**Status: v3.5.1 shipped live ISO-TP verify and extra log PIDs. This pass closes the two remaining code bugs that made the app hang or write Bosch/ME7 images over VPW.**

## What this pass actually changed

1. `read_properties` no longer re-locks `STATE` while `with_port` already holds it (deadlock on VIN/CALID read).
2. Guided flash uses UDS 0x34/36/37 `download_image` for EDC / MED / ME7 / Delphi / DCM. P01/P59 stay on VPW Mode 34/36/37.
3. Cal start address is family-aware (ME7 0x18000, Bosch/Delphi 0x80000, P01 0x20000).
4. Unlock L1/L2 offline paths return `success: false` instead of a raw Tauri error.
5. Heatmap cells tint when STFT average is available from the log.
6. Versions synced to 3.6.0.

## Still needs your bench

1. EDC17 / MED17 / ME7 seed tables from **your** dumps.
2. PCM Hammer comparison on your 512 KB P01 OS.
3. A vendor J2534 DLL on Windows to confirm the registry walk.
4. A kernel-resident Mode 3C full-image dump. Windowed probes are not a full backup.
5. ME7 block checksum routine measured on a personal dump before a corrector ships.

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
