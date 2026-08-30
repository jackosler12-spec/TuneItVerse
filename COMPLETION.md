# TuneItVerse v3.2.0 — Honest operational pass (2026-08-30)

**Status: first v3.2.0 slice is on this branch. Larger module rewrites are staged locally.**

## What is actually on this branch

- Identify reports **all** families that share a BIN size (512 KB → P01 and P59; 2 MB → EDC16 / EDC17 / MED17).
- Version bump to 3.2.0 in package.json / Cargo.toml / tauri.conf / README.
- Docs stop pretending v3.1.0 implemented 512 KB P01 checksums. It did not.

## What v3.1.0 still gets wrong on main (local fixes exist, not all uploaded yet)

These are real bugs found by reading `main`, not a feature wishlist:

1. `checksum.rs` still rejects anything except 131072 bytes. Real LS1 dumps are 524288.
2. `GuidedFlashRequest.user_confirmed_risks` defaults to **true**.
3. Guided flash auto-correct only runs on 128 KB images.
4. Hex dump UI starts at `0x20000` instead of the selected table address.
5. BIN load does not call `identify_bin_cmd`.
6. Scripts tab lists logging templates, not `list_script_helpers`.
7. Flash UI omits `user_confirmed_risks`.
8. Browser mock flash returns `success: true`.
9. J2534 device list is a hardcoded string, not a registry walk.
10. Bosch unlock offline path returns `success: true`.

Local workspace (`/home/workdir/TuneItVerse`) contains the 512 KB checksum path, fail-closed flash default, CSV import, J2534 registry walk, compute_seed_key, and UI wiring. Those files are large; they need a follow-up commit on this branch before you treat flash/CS as fixed.

## Still needs your bench

1. EDC17 / MED17 seed tables from your dumps
2. PCM Hammer comparison on your 512 KB P01 OS
3. Windows box + vendor J2534 DLL

Never flash without a verified backup and stable power. Personal dumps only.

Build your own. No bullshit prices.
