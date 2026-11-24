# GM multi-family checksums and the local definitions folder (v3.64.0)

## What it does

- `src-tauri/src/gm_checksums.rs` identifies and verifies the stored segment
  checksums of ten GM families. It can also compute corrected checksum words
  **in memory**, so you can export a corrected copy with Save As.
- `src-tauri/src/local_defs.rs` loads TunerPro XDFs from a folder you choose on
  your own disk. A file loads only if its SHA-256 is listed in
  `src-tauri/src/local_defs_manifest.json`.

None of this touches an ECU, and none of it changes a write gate. `flash.rs` is
not modified. P01 keeps its existing experimental write path, which is off by
default and gated separately. Every other family listed here can only be
identified and verified: there is no flash path for it.

## Families

All checksums are stored big-endian. Ranges are inclusive. "Pair @X" means a
u32 start/end pair read from the image at X, so the segment bounds come from
the image's own pointer table and are never assumed. The checksum field is
always skipped when it falls inside a covered block.

| Family | Full image | Checksums | Layout facts |
|---|---|---|---|
| P01 (P59 shares the layout) | 512KB | 8 | Uses the existing `checksum::p01_segments` reader. OS word sum (two's complement) at 0x500 over 0-0x4FF, 0x502-0x3FFF, 0x20000-[u32 @0x510]. Seven cal segments come from pairs @0x514+8n, with the checksum word at each segment start. The CRC-16 CVN (CS2) is not stored in the image. |
| P08/P10 | 512KB | 5 | OS word sum (two's complement) at 0x52A over 0-0x529, 0x52C-0x3FFF, 0x20000-(end-4). Cal segments come from pairs @0x54E/0x556/0x55E/0x566. |
| P04 3800 V6 | 512KB | 1 | 32-bit word sum, no complement, over 0-0x3FFF and 0x8000-end. The 4-byte OS id just before the 0xA55A marker near the end is excluded. The checksum address is read from the unique 68k `move.l (0x7FFFA).l` load and the matching opcode 16-32 bytes later (0x6FBFA on OS 12201830). |
| LS1 97-98 / P30 | 512KB | 1 | 32-bit word sum, no complement, at 0x6F294 over 0-0x3FFF, 0x8000-0x6F293, 0x6F298-0x6F2B7 and 0x6F2BC-end. OS id at 0x6F2B8. |
| E38 | 2MB | 12 (14 with boot block) | Segment headers are pairs @0x10024, 0x10048, 0x1006B, 0x1008E, 0x100B1, 0x100D4. In each: CRC-16/ARC (byte-swapped) at +0x1E over pairs @X+8 and @X+16, and the word sum (two's complement) at +0 over pair @X+24. The boot block (pair @0x414) is checked only when the image contains it. |
| E92 / E92A | 4MB | 12 (+2 with boot block) | OS pair @0xC0126: CRC at +0x120 over 3 pairs @0xC0136, word sum at +0x100 over 2 pairs @0xC014E. Cal headers at 0xC0162, 0xC0186, 0xC01AA, 0xC01CE, 0xC01F2: CRC at +0x20 over @X+8 and @X+16, word sum at +0 over @X+24. Boot block pair @0xFDA5, checked when present. |
| T43 TCM | 2MB | 8 (+2 with boot block) | OS pair @0x60022: CRC at +0x3001C over 4 pairs @0x60032, word sum at +0x30000 over 3 pairs @0x60052. Cal headers at 0x6006E, 0x60091, 0x600B4: CRC at +0x1C, word sum at +0. Boot block pair @0x2A022, checked when present. |
| Duramax E54 LB7 | 512KB | 6 | OS pair @0x2000A, word at 0x20000. Cal pairs @0x20012 + 8n (5 segments). OS id at 0x20004. |
| Duramax E60 LLY | 1MB | 7 | OS pair @0x4000A, word at 0x40000. Cal pairs @0x40012 + 8n (6 segments). OS id at 0x40004. |
| 96-97 Vortec blackbox | 256KB | 7 | OS pair @0x40C, word at 0x400. Cal pairs @0x414 + 8n (6 segments). OS id at 0x404. |

Short files are rebuilt to the full layout before checking:

- TunerCat P01/P08 files (0x7C000 bytes)
- TunerCat E54 files (0x68000 bytes)
- TunerCat E60 files (0xA8000 bytes)
- E38, T43 and E92 reads that lack the boot block (padded with 0xFF)

The exported corrected copy uses the full layout, not the short TunerCat
layout.

## Tests

- Unit tests use synthetic images only, with no calibration data:
  - known CRC vectors
  - hand-derived sum and complement vectors
  - corrupt, then repair, for every family
  - a missing boot block is reported rather than guessed
  - short-file rebuild
  - P01 results agree with the existing `checksum.rs` validator and corrector, byte for byte
- `corpus_reproduces_stored_checksums_when_present` is opt-in. Set
  `TIV_GM_CS_CORPUS` to a folder of stock images. Files are matched by SHA-256
  only; the images and their file names are never committed. With the 33
  reference images it reproduces **267/267** stored checksums. It also runs
  corrupt-then-fail and fix-then-pass on every image.
- `p01_private_dump_agrees_when_present` uses `TIV_P01_BIN`, like the existing
  tests do.
- `real_defs_folder_when_present` is opt-in via `TIV_LOCAL_DEFS_DIR`.

## Local definitions folder

- The manifest holds only metadata for each file: SHA-256, file name, size,
  family, OS ids, tier, item count, source, licence and notes.
- These are refused:
  - files not in the manifest
  - files whose name matches an entry but whose hash does not
  - symlinks, paths outside the folder, and files over 32MB
- The hash is checked again at load time.
- Table flags are taken from each description:
  - `[confirmed]` marks a table as confirmed.
  - `[unconfirmed]` marks it unconfirmed and read-only.
  - Heuristic tier files and `UNCONFIRMED ...` candidates are unconfirmed and
    read-only, even though those generated descriptions also say "[confirmed]
    source: heuristic scan".
  - P04 tables whose row/column order was inferred by smoothness are
    read-only. That is all 70 of them: 43 chosen as column-major and 27 left
    row-major.
- Read-only travels as `patch_blocked`, so `patch_table_into_bin` refuses an
  edit.

## Provenance and licensing (judgment calls for review)

- The checksum algorithms and layouts were first described publicly by
  Universal Patcher (github.com/joukoy/UniversalPatcher, GPL-3.0). This
  repository does not contain any of its config files, code, TableSeek packs
  or text. The Rust above restates functional facts (offsets, widths and
  algorithm parameters) needed for interoperability with the ECU images. They
  were written fresh and verified against stock images. Treating these facts as
  non-copyrightable is a judgment call. Review it before redistributing
  binaries.
- The XDFs are never committed. Their own licences are recorded in the
  manifest:
  - TableSeek-derived XDFs come from GPL-3.0 packs.
  - The P10 "Phoenix" XDF and the hkaase 12225074 XDF state no licence, so
    treat them as all rights reserved.
  - TinyTuner P04 says "use without limitation".
  - The heuristic XDFs have no third-party content.
- `reference/tableseek-p01-p59.xml` (a GPL-3.0 Universal Patcher file) was
  already embedded before this change. That is a pre-existing licensing
  question for the MIT repository, and this change does not alter it.

## Not done (follow-ups)

- TableSeek scanner token bug (`rNN` / `kNN` / `#NN` tokens misread in
  `tableseek.rs`). This was reported by the definitions work but was not in
  scope here.
- P59 1MB images are not checked: `p01_segments` requires 512KB, and no P59
  image was available to verify against.
- LT1/LT4 96-97 and the Ford images: checksums unknown, no definitions.
- EFILive `.tun`-derived Duramax files are obfuscated. They are not raw flash
  and must not be loaded as images.
- Exporting a corrected copy back into the short TunerCat layout.
- Hardware testing. None of this has been run against a vehicle or a bench ECU.
