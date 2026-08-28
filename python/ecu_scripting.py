#!/usr/bin/env python3
"""TuneItVerse bench helpers.

This is NOT an embedded interpreter. The desktop app checksum / identify /
map-from-log path lives in the Rust backend. Use this file on the bench when
you want a quick CLI check of a personal dump.

Usage:
  python3 python/ecu_scripting.py checksum path/to/dump.bin
  python3 python/ecu_scripting.py identify path/to/dump.bin
"""

from __future__ import annotations

import argparse
import pathlib
import sys
import zlib


P01_128 = 131072
P01_512 = 524288
EDC16 = 2097152
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
        P01_512: "P01_0411",
        EDC16: "EDC16/EDC17/MED17 2MB",
    }.get(size, "unknown")
    return {"bytes": size, "family_by_size": family}


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
        lines.append("Unsupported size. P01 128/512KB or EDC16 2MB.")
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="TuneItVerse bench helper")
    parser.add_argument("command", choices=["checksum", "identify"])
    parser.add_argument("bin_path")
    args = parser.parse_args(argv)
    path = pathlib.Path(args.bin_path)
    data = path.read_bytes()
    if args.command == "identify":
        print(identify(data))
    else:
        print(checksum_report(data))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
