# Reference Files

This folder holds ECU reference files used by Perplexity Computer to build
accurate Stage 2 address maps, checksum offsets, and calibration table boundaries.

## Required files

| File | Description |
|---|---|
| `p01_12225074.bin` | Full 512 KB P01 0411 PCM flash backup (OSID 12225074) |
| `12225074.xdf` | TunerPro XDF definition for OSID 12225074 |

## How to add your files

1. Open [github.com/jackosler12-spec/TuneItVerse/tree/reference-files/reference](https://github.com/jackosler12-spec/TuneItVerse/tree/reference-files/reference)
2. Click **Add file → Upload files**
3. Drag and drop your `.bin` and `.xdf` files
4. Set commit target to **reference-files** branch
5. Click **Commit changes**

Once uploaded, tell Perplexity Computer — it will read them via the GitHub API
and extract the address map for Stage 2.

## What gets extracted

- **From `.bin`:** OSID bytes (offset `0x57F40`), VIN region, cal region start/end,
  checksum byte locations, OS code region boundaries
- **From `.xdf`:** Named calibration table addresses + sizes, axis scaling,
  checksum plugin offsets, security seed-key reference

## Security note

This branch is for development reference only. Do **not** commit files containing
personal VIN numbers to a public repository. Scrub the VIN region of the `.bin`
before uploading if privacy is a concern (bytes `0x57F58`–`0x57F6C` on P01).
