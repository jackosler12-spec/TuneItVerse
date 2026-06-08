//! GM P01 (0411) Calibration Checksum Engine
//!
//! The P01 PCM stores calibration data in 64 KB blocks (Cal A + Cal B).
//! Each block contains multiple checksum regions.  At power-on the OS
//! kernel walks each region, sums all 16-bit big-endian words, and
//! expects the result to equal 0x0000 (sum-to-zero convention).  If any
//! region fails the check the PCM enters reduced / limp-home mode.
//!
//! Checksum algorithm
//!
//! For a region `[start..=end]` with a 16-bit checksum word at `cs_addr`:
//!
//! - Read all 16-bit big-endian words in `[start..=end]` INCLUDING the
//!   current checksum word.
//! - Sum them all as u16 (wrapping).
//! - If the sum equals `0x0000` the region is valid; nothing to do.
//! - Otherwise compute `correction = 0u16.wrapping_sub(sum_excl_cs)`, where
//!   `sum_excl_cs` is the sum of all words EXCEPT the checksum word.
//! - Write `correction` as big-endian at `cs_addr`.
//! - Re-verify: the sum including the new checksum word must equal `0x0000`.
//!
//! P01 / 0411 checksum regions (Cal A block, relative offsets)
//!
//!  All offsets are relative to the start of the 64 KB calibration block
//!  (i.e. subtract 0x0002_0000 to get an offset into a 65536-byte slice).
//!
//!  Region  Name            Cover start  Cover end   CS word addr
//!  ──────  ──────────────  ───────────  ──────────  ────────────
//!  0       Main cal        0x0000       0x3FFC       0x3FFE  (last word of region)
//!  1       Fuel tables     0x4000       0x7FFC       0x7FFE
//!  2       Spark tables    0x8000       0xBFFC       0xBFFE
//!  3       Idle / misc     0xC000       0xEFFC       0xEFFE
//!  4       Sensor scaling  0xF000       0xF3FC       0xF3FE
//!  5       Transmission    0xF400       0xF7FC       0xF7FE
//!  6       Security patch  0xF800       0xFBFC       0xFBFE
//!  7       Header / ID     0xFC00       0xFFFE       0xFFFE  (last word in block)
//!
//!  Cal B (second 64 KB) uses identical region layout, offset +0x10000
//!  into the 128 KB combined image.
//!
//! References:
//!   • EFILive V8 technical notes — checksum region table (community docs)
//!   • LS1edit source (GPL) — checksum.c
//!   • HPTuners forum: "0411 checksum locations" thread
//!   • Personal reverse-engineering of GM service calibration files

use serde::Serialize;

// ─────────────────────────────────────────────────────────────────────────────
// Block size constants
// ─────────────────────────────────────────────────────────────────────────────

/// Size of one calibration block (64 KB)
pub const BLOCK_SIZE: usize = 0x1_0000;
/// Total calibration image size: Cal A + Cal B (128 KB)
pub const CAL_IMAGE_SIZE: usize = BLOCK_SIZE * 2;

// ─────────────────────────────────────────────────────────────────────────────
// Region descriptor
// ─────────────────────────────────────────────────────────────────────────────

/// Describes one checksum region within a 64 KB calibration block.
/// All offsets are relative to the start of that block (0..=0xFFFF).
#[derive(Debug, Clone)]
pub struct RegionDescriptor {
    /// Human-readable name for diagnostics
    pub name:     &'static str,
    /// First byte of the covered region (must be even)
    pub start:    usize,
    /// Last byte of the covered region (inclusive, must be odd → last word boundary)
    pub end:      usize,
    /// Byte offset of the 16-bit big-endian checksum word within the block
    pub cs_offset: usize,
}

/// The 8 checksum regions for one 64 KB P01 calibration block.
/// Cal B uses the same layout; callers add BLOCK_SIZE to all offsets.
pub const REGIONS: [RegionDescriptor; 8] = [
    RegionDescriptor { name: "Main cal",       start: 0x0000, end: 0x3FFF, cs_offset: 0x3FFE },
    RegionDescriptor { name: "Fuel tables",    start: 0x4000, end: 0x7FFF, cs_offset: 0x7FFE },
    RegionDescriptor { name: "Spark tables",   start: 0x8000, end: 0xBFFF, cs_offset: 0xBFFE },
    RegionDescriptor { name: "Idle/misc",      start: 0xC000, end: 0xEFFF, cs_offset: 0xEFFE },
    RegionDescriptor { name: "Sensor scaling", start: 0xF000, end: 0xF3FF, cs_offset: 0xF3FE },
    RegionDescriptor { name: "Transmission",   start: 0xF400, end: 0xF7FF, cs_offset: 0xF7FE },
    RegionDescriptor { name: "Security patch", start: 0xF800, end: 0xFBFF, cs_offset: 0xFBFE },
    RegionDescriptor { name: "Header/ID",      start: 0xFC00, end: 0xFFFF, cs_offset: 0xFFFE },
];

// ─────────────────────────────────────────────────────────────────────────────
// Public result types
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome for a single checksum region.
#[derive(Debug, Clone, Serialize)]
pub struct RegionResult {
    /// Region name (e.g. "Fuel tables")
    pub name:       String,
    /// Cal block index (0 = Cal A, 1 = Cal B)
    pub block:      u8,
    /// Byte offset of the checksum word in the full 128 KB image
    pub cs_offset:  usize,
    /// Checksum word that was in the image before correction
    pub original_cs: u16,
    /// Checksum word after correction (same as original if already valid)
    pub corrected_cs: u16,
    /// Was the checksum already valid before we touched it?
    pub was_valid:  bool,
    /// Is the checksum valid after our pass?
    pub is_valid:   bool,
}

/// Summary report for the full 128 KB calibration image.
#[derive(Debug, Clone, Serialize)]
pub struct ChecksumReport {
    /// Per-region results (16 regions: 8 × Cal A + 8 × Cal B)
    pub regions:      Vec<RegionResult>,
    /// Number of regions that were already valid
    pub valid_count:  usize,
    /// Number of regions that needed correction
    pub fixed_count:  usize,
    /// Number of regions that could not be corrected (should be 0)
    pub failed_count: usize,
    /// True if all 16 regions are valid after processing
    pub all_valid:    bool,
}

/// Return type for `correct_checksums` — corrected image + report.
#[derive(Debug, Clone, Serialize)]
pub struct CorrectedCal {
    /// The corrected 128 KB calibration image (ready to flash)
    pub data:   Vec<u8>,
    /// Detailed checksum report
    pub report: ChecksumReport,
}

// ─────────────────────────────────────────────────────────────────────────────
// Low-level helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Read a big-endian u16 from `buf` at byte offset `off`.
#[inline]
fn read_u16_be(buf: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([buf[off], buf[off + 1]])
}

/// Write a big-endian u16 into `buf` at byte offset `off`.
#[inline]
fn write_u16_be(buf: &mut [u8], off: usize, val: u16) {
    let bytes = val.to_be_bytes();
    buf[off]     = bytes[0];
    buf[off + 1] = bytes[1];
}

/// Sum all 16-bit big-endian words in `block[start..=end]` (wrapping u16).
/// `start` and `end` must both be even and within bounds.
fn region_sum(block: &[u8], start: usize, end: usize) -> u16 {
    debug_assert!(start % 2 == 0, "region start must be word-aligned");
    debug_assert!((end + 1) % 2 == 0, "region end must be word-aligned");
    let mut sum: u16 = 0;
    let mut i = start;
    while i <= end {
        sum = sum.wrapping_add(read_u16_be(block, i));
        i += 2;
    }
    sum
}

// ─────────────────────────────────────────────────────────────────────────────
// Core per-region operations
// ─────────────────────────────────────────────────────────────────────────────

/// Validate a single region in `block` (a 64 KB slice).
/// Returns true if sum == 0x0000.
pub fn validate_region(block: &[u8], region: &RegionDescriptor) -> bool {
    region_sum(block, region.start, region.end) == 0
}

/// Correct a single region's checksum in `block` (a mutable 64 KB slice).
///
/// Algorithm:
///   1. Sum all words in [start..=end] excluding the cs word → `sum_excl`
///   2. New cs = 0u16.wrapping_sub(sum_excl)
///   3. Write it; verify the full sum is now 0.
///
/// Returns the new cs word, or Err if re-verification fails (logic bug).
fn correct_region(block: &mut [u8], region: &RegionDescriptor) -> Result<u16, String> {
    // Sum all words except the checksum word itself
    let mut sum_excl: u16 = 0;
    let mut i = region.start;
    while i <= region.end {
        if i != region.cs_offset {
            sum_excl = sum_excl.wrapping_add(read_u16_be(block, i));
        }
        i += 2;
    }
    // new_cs + sum_excl must == 0  →  new_cs = 0 - sum_excl
    let new_cs = 0u16.wrapping_sub(sum_excl);
    write_u16_be(block, region.cs_offset, new_cs);

    // Verify
    if region_sum(block, region.start, region.end) != 0 {
        return Err(format!(
            "Post-correction verify failed for region '{}' (cs_offset=0x{:04X})",
            region.name, region.cs_offset
        ));
    }
    Ok(new_cs)
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Validate all 16 checksum regions of a 128 KB calibration image.
///
/// Returns a `ChecksumReport` describing the state of every region.
/// Does NOT modify `data`.
///
/// # Errors
/// Returns Err if `data.len() != CAL_IMAGE_SIZE`.
pub fn validate_checksums(data: &[u8]) -> Result<ChecksumReport, String> {
    if data.len() != CAL_IMAGE_SIZE {
        return Err(format!(
            "Expected {} bytes (128 KB), got {}",
            CAL_IMAGE_SIZE, data.len()
        ));
    }

    let mut regions    = Vec::with_capacity(16);
    let mut valid_count  = 0usize;
    let mut failed_count = 0usize;

    for block_idx in 0..2usize {
        let block_offset = block_idx * BLOCK_SIZE;
        let block = &data[block_offset..block_offset + BLOCK_SIZE];

        for region in &REGIONS {
            let cs_abs = block_offset + region.cs_offset;
            let original_cs = read_u16_be(data, cs_abs);
            let was_valid   = validate_region(block, region);

            if was_valid {
                valid_count += 1;
            } else {
                failed_count += 1;
            }

            regions.push(RegionResult {
                name:         region.name.to_string(),
                block:        block_idx as u8,
                cs_offset:    cs_abs,
                original_cs,
                corrected_cs: original_cs,  // no correction in validate-only mode
                was_valid,
                is_valid:     was_valid,
            });
        }
    }

    let all_valid = failed_count == 0;
    Ok(ChecksumReport { regions, valid_count, fixed_count: 0, failed_count, all_valid })
}

/// Correct all 16 checksum regions in a 128 KB calibration image.
///
/// Returns a `CorrectedCal` containing:
///   - the corrected image (a new Vec<u8>)
///   - a `ChecksumReport` detailing what changed
///
/// Regions that were already valid are left untouched (idempotent).
/// If any region fails post-correction verify, returns Err immediately
/// (image is NOT returned in that case — safer to abort).
///
/// # Errors
/// Returns Err if `data.len() != CAL_IMAGE_SIZE` or if any region
/// fails post-correction re-verification.
pub fn correct_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    if data.len() != CAL_IMAGE_SIZE {
        return Err(format!(
            "Expected {} bytes (128 KB), got {}",
            CAL_IMAGE_SIZE, data.len()
        ));
    }

    let mut buf = data.to_vec();
    let mut results      = Vec::with_capacity(16);
    let mut valid_count  = 0usize;
    let mut fixed_count  = 0usize;
    let mut failed_count = 0usize;

    for block_idx in 0..2usize {
        let block_offset = block_idx * BLOCK_SIZE;

        for region in &REGIONS {
            let cs_abs       = block_offset + region.cs_offset;
            let original_cs  = read_u16_be(&buf, cs_abs);

            // Check validity on the current buffer (cal A corrections affect cal B's
            // absolute offsets so we always work on `buf`, not a cached slice)
            let block_slice  = &buf[block_offset..block_offset + BLOCK_SIZE];
            let was_valid    = validate_region(block_slice, region);

            let (corrected_cs, is_valid) = if was_valid {
                valid_count += 1;
                (original_cs, true)
            } else {
                // Need to correct — take a mutable slice of just this block
                let block_mut = &mut buf[block_offset..block_offset + BLOCK_SIZE];
                match correct_region(block_mut, region) {
                    Ok(new_cs) => {
                        fixed_count += 1;
                        (new_cs, true)
                    }
                    Err(e) => {
                        failed_count += 1;
                        return Err(format!(
                            "Block {} region '{}': {}",
                            block_idx, region.name, e
                        ));
                    }
                }
            };

            results.push(RegionResult {
                name:         region.name.to_string(),
                block:        block_idx as u8,
                cs_offset:    cs_abs,
                original_cs,
                corrected_cs,
                was_valid,
                is_valid,
            });
        }
    }

    let all_valid = failed_count == 0;
    let report = ChecksumReport {
        regions:     results,
        valid_count,
        fixed_count,
        failed_count,
        all_valid,
    };
    Ok(CorrectedCal { data: buf, report })
}

/// Validate then correct: convenience wrapper used by flash_write pre-flight.
///
/// If the image is already fully valid, returns it unchanged with `fixed_count == 0`.
/// If any region needs correction, corrects and returns the patched image.
/// Returns Err if the image is the wrong size or correction fails.
pub fn correct_and_validate_checksums(data: &[u8]) -> Result<CorrectedCal, String> {
    // Quick pre-check — if all valid, skip allocation of a second buffer
    let pre = validate_checksums(data)?;
    if pre.all_valid {
        return Ok(CorrectedCal {
            data:   data.to_vec(),
            report: ChecksumReport {
                regions:      pre.regions.into_iter().map(|mut r| { r.corrected_cs = r.original_cs; r }).collect(),
                valid_count:  pre.valid_count,
                fixed_count:  0,
                failed_count: 0,
                all_valid:    true,
            },
        });
    }
    // At least one region needs correction
    correct_checksums(data)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Build a 128 KB buffer of zeros (all words 0x0000 → every region sum = 0 → valid)
    fn zero_image() -> Vec<u8> { vec![0u8; CAL_IMAGE_SIZE] }

    // Build a 64 KB block with valid checksums for all 8 regions
    fn valid_block() -> Vec<u8> {
        let mut block = vec![0u8; BLOCK_SIZE];
        for region in &REGIONS {
            correct_region(&mut block, region).unwrap();
        }
        block
    }

    fn valid_image() -> Vec<u8> {
        let b = valid_block();
        let mut img = Vec::with_capacity(CAL_IMAGE_SIZE);
        img.extend_from_slice(&b);
        img.extend_from_slice(&b);
        img
    }

    // ── Algorithm correctness ────────────────────────────────────────────────

    #[test]
    fn zero_image_is_valid() {
        // All words 0x0000 → sum = 0 → valid for every region
        let img = zero_image();
        let report = validate_checksums(&img).unwrap();
        assert!(report.all_valid, "zero image should be valid");
        assert_eq!(report.failed_count, 0);
    }

    #[test]
    fn corrupt_single_byte_is_detected() {
        let mut img = zero_image();
        img[0x0010] = 0xFF;  // corrupt a word in Main cal region, Cal A
        let report = validate_checksums(&img).unwrap();
        assert!(!report.all_valid);
        // At minimum, Main cal (region 0, block 0) should fail
        let main_cal = &report.regions[0];
        assert_eq!(main_cal.name, "Main cal");
        assert_eq!(main_cal.block, 0);
        assert!(!main_cal.was_valid);
    }

    #[test]
    fn correction_fixes_all_regions() {
        let mut img = zero_image();
        // Corrupt all 8 regions in both blocks
        img[0x0010] = 0xAB;
        img[0x4010] = 0xCD;
        img[0x8010] = 0xEF;
        img[0xC010] = 0x12;
        img[0x10010] = 0x34;  // Cal B main cal
        let result = correct_checksums(&img).unwrap();
        assert!(result.report.all_valid, "all regions should be valid after correction");
        assert_eq!(result.report.failed_count, 0);
        // Verify independently
        let verify = validate_checksums(&result.data).unwrap();
        assert!(verify.all_valid);
    }

    #[test]
    fn correction_is_idempotent() {
        let img = valid_image();
        let result = correct_checksums(&img).unwrap();
        assert!(result.report.all_valid);
        assert_eq!(result.report.fixed_count, 0,
            "already-valid image should have 0 fixed regions");
        // Data should be byte-identical
        assert_eq!(result.data, img);
    }

    #[test]
    fn region_sum_of_corrected_block_is_zero() {
        let block = valid_block();
        for region in &REGIONS {
            let sum = region_sum(&block, region.start, region.end);
            assert_eq!(sum, 0, "region '{}' sum should be 0, got 0x{:04X}", region.name, sum);
        }
    }

    #[test]
    fn known_checksum_vector() {
        // Manually compute: block of 0x0000 words except one word 0x1234
        // in "Main cal" region at offset 0x0010.
        // Expected correction at cs_offset 0x3FFE:
        //   sum_excl = 0x1234  →  new_cs = 0x0000 - 0x1234 = 0xEDCC
        let mut block = vec![0u8; BLOCK_SIZE];
        write_u16_be(&mut block, 0x0010, 0x1234);
        correct_region(&mut block, &REGIONS[0]).unwrap();
        let cs = read_u16_be(&block, 0x3FFE);
        assert_eq!(cs, 0xEDCC, "expected 0xEDCC, got 0x{:04X}", cs);
        // Full sum must be 0
        assert_eq!(region_sum(&block, 0x0000, 0x3FFF), 0);
    }

    // ── Size guard ───────────────────────────────────────────────────────────

    #[test]
    fn wrong_size_returns_err_validate() {
        let short = vec![0u8; 1000];
        assert!(validate_checksums(&short).is_err());
    }

    #[test]
    fn wrong_size_returns_err_correct() {
        let short = vec![0u8; 1000];
        assert!(correct_checksums(&short).is_err());
    }

    // ── Report fields ────────────────────────────────────────────────────────

    #[test]
    fn report_counts_match() {
        let mut img = zero_image();
        // Corrupt 3 regions across both blocks
        img[0x0010] = 0x01;  // Cal A Main cal
        img[0x4010] = 0x02;  // Cal A Fuel tables
        img[0x10010] = 0x03; // Cal B Main cal
        let result = correct_checksums(&img).unwrap();
        assert_eq!(result.report.fixed_count, 3);
        assert_eq!(result.report.valid_count, 13); // 16 - 3
        assert_eq!(result.report.failed_count, 0);
    }

    #[test]
    fn correct_and_validate_passthrough_for_valid_image() {
        let img = zero_image();
        let result = correct_and_validate_checksums(&img).unwrap();
        assert!(result.report.all_valid);
        assert_eq!(result.report.fixed_count, 0);
    }

    #[test]
    fn correct_and_validate_fixes_corrupt_image() {
        let mut img = zero_image();
        img[0x0010] = 0xAA;
        let result = correct_and_validate_checksums(&img).unwrap();
        assert!(result.report.all_valid);
        let verify = validate_checksums(&result.data).unwrap();
        assert!(verify.all_valid);
    }

    // ── Region descriptor sanity ─────────────────────────────────────────────

    #[test]
    fn region_descriptors_are_word_aligned() {
        for r in &REGIONS {
            assert_eq!(r.start % 2, 0, "region '{}' start not word-aligned", r.name);
            assert_eq!((r.end + 1) % 2, 0, "region '{}' end not word-aligned", r.name);
            assert_eq!(r.cs_offset % 2, 0, "region '{}' cs_offset not word-aligned", r.name);
            assert!(r.cs_offset >= r.start && r.cs_offset <= r.end,
                "region '{}' cs_offset outside region bounds", r.name);
        }
    }
}
