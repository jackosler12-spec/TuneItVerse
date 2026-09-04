#!/usr/bin/env python3
"""TuneItVerse bench helpers.

This is NOT an embedded interpreter. The desktop app checksum / identify /
map-from-log path lives in the Rust backend. Use this file on the bench when
you want a quick CLI check of a personal dump.

Usage:
  python3 python/ecu_scripting.py checksum path/to/dump.bin
  python3 python/ecu_scripting.py identify path/to/dump.bin
  python3 python/ecu_scripting.py seedkey P01_0411 1234 1
"""

from __future__ import annotations

import argparse
import pathlib
import sys
import hashlib
import zlib


P01_128 = 131072
P01_512 = 524288
EDC16 = 2097152
SID803 = 1572864
BLOCK = 0x10000

P01_REGIONS = [
    ("Main cal", 0x0000, 0x3FFF, 0x3FFE),
    ("Fuel tables", 0x4000, 0x7FFF, 0x7FFE),
    ("Spark tables", 0x8000, 0xBFFF, 0xBFFE),
    ("Idle/misc", 0xC000, 0xEFFF, 0xEFFE),
    ("Sensor scaling", 0xF000, 0xF3FF, 0xF3FE),
    ("Transmission", 0xF400, 0xF7FF, 0xF7FE),
    ("Security patch", 0xF800, 0xFBFF, 0xFBFE),
    ("Header/ID", 0xFC00, 0xFFFF, 0xFFFE),
]


def sum16_be(block: bytes, start: int, end: int) -> int:
    total = 0
    i = start
    while i + 1 <= end and i + 1 < len(block):
        total = (total + ((block[i] << 8) | block[i + 1])) & 0xFFFF
        i += 2
    return total


def identify(data: bytes) -> dict:
    size = len(data)
    family = {
        P01_128: "P01_0411 (128KB slice)",
        P01_512: "P01_0411 or HONDA_KEIHIN — confirm OS string",
        1048576: "ME7_COMMON",
        EDC16: "EDC16/EDC17/MED17/DELPHI 2MB",
        SID803: "SIEMENS_SID803 (1.5MB)",
    }.get(size, "unknown")
    return {
        "bytes": size,
        "family_by_size": family,
        "sha256": hashlib.sha256(data).hexdigest(),
        "sha256_head_4k": hashlib.sha256(data[:4096]).hexdigest() if data else None,
        "sha256_tail_4k": hashlib.sha256(data[-4096:]).hexdigest() if data else None,
    }


def checksum_report(data: bytes) -> str:
    info = identify(data)
    lines = [f"size={info['bytes']} family={info['family_by_size']}"]
    if len(data) in (P01_128, P01_512):
        blocks = len(data) // BLOCK
        for b in range(blocks):
            chunk = data[b * BLOCK : (b + 1) * BLOCK]
            for name, start, end, _cs in P01_REGIONS:
                s = sum16_be(chunk, start, end)
                lines.append(f"  blk{b} {name}: sum16=0x{s:04X} {'OK' if s == 0 else 'BAD'}")
        if len(data) == P01_512:
            lines.append("Note: 512KB also matches Honda Keihin. Confirm 37820-* vs 12225074 before using P01 correction.")
    elif len(data) == 1048576:
        lines.append("ME7 1MB catalogued. No verified corrector in this CLI — do not invent CS bytes.")
    elif len(data) == SID803:
        lines.append("SID803 1.5MB catalogued. Report-only — no invented corrector.")
    elif len(data) == EDC16:
        regions = [
            (0x00000, 0x1FFFF),
            (0x20000, 0x7FFFF),
            (0x80000, 0xBFFFF),
            (0xC0000, 0xFFFFF),
            (0x100000, 0x13FFFF),
            (0x140000, 0x17FFFF),
            (0x180000, 0x1FFFFF),
        ]
        for start, end in regions:
            crc = zlib.crc32(data[start : end + 1]) & 0xFFFFFFFF
            lines.append(f"  0x{start:06X}-0x{end:06X} crc32=0x{crc:08X}")
        lines.append("Note: live correction is in src-tauri/src/checksum.rs")
    else:
        lines.append("Unsupported size for correction. Report-only.")
    return "\n".join(lines)


def p01_key(seed: int, level: int) -> int:
    if seed == 0:
        return 0
    poly = 0x8005
    lo_nibble = seed & 0x000F
    base = 5 if level == 2 else 3
    lfsr = seed & 0xFFFF
    for _ in range(lo_nibble + base):
        if lfsr & 0x8000:
            lfsr = ((lfsr << 1) ^ poly) & 0xFFFF
        else:
            lfsr = (lfsr << 1) & 0xFFFF
    if level == 2:
        lfsr ^= 0x36A9
    return lfsr


def edc16c41_key(seed: int) -> int:
    key = seed & 0xFFFFFFFF
    key ^= 0xA5C3B7D9
    key = ((key << 5) | (key >> 27)) & 0xFFFFFFFF
    key = (key + 0x12345678) & 0xFFFFFFFF
    key ^= 0x87654321
    return int.from_bytes(key.to_bytes(4, "big")[::-1], "big")


def seedkey(family: str, seed_hex: str, level: str) -> dict:
    cleaned = "".join(c for c in seed_hex if c in "0123456789abcdefABCDEF")
    if len(cleaned) < 2 or len(cleaned) % 2:
        raise SystemExit("seed hex must be even-length")
    seed_bytes = bytes.fromhex(cleaned)
    fam = family.upper()
    if "P01" in fam or "P59" in fam or fam.startswith("GM"):
        seed = int.from_bytes(seed_bytes[:2], "big")
        key = p01_key(seed, 2 if level in {"2", "flash", "level2"} else 1)
        return {"family": family, "algo": "p01_lfsr16", "seed_hex": cleaned.upper(), "key_hex": f"{key:04X}"}
    if len(seed_bytes) >= 4 and "EDC16" in fam:
        seed = int.from_bytes(seed_bytes[:4], "big")
        key = edc16c41_key(seed)
        return {"family": family, "algo": "edc16c41", "seed_hex": cleaned.upper(), "key_hex": f"{key:08X}"}
    return {"family": family, "algo": "unsupported-in-cli", "seed_hex": cleaned.upper(), "key_hex": None}


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="TuneItVerse bench helper")
    parser.add_argument("command", choices=["checksum", "identify", "seedkey"])
    parser.add_argument("bin_path", nargs="?")
    parser.add_argument("seed_hex", nargs="?")
    parser.add_argument("level", nargs="?", default="1")
    args = parser.parse_args(argv)
    if args.command == "seedkey":
        family = args.bin_path or "P01_0411"
        seed = args.seed_hex or ""
        print(seedkey(family, seed, args.level))
        return 0
    if not args.bin_path:
        raise SystemExit("bin_path required")
    path = pathlib.Path(args.bin_path)
    data = path.read_bytes()
    if args.command == "identify":
        print(identify(data))
    else:
        print(checksum_report(data))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
