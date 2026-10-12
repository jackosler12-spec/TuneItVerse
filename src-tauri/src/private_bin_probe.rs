//! Optional checks against a real 512KB P01 (OS 12225074) read.
//! Set TIV_P01_BIN to the image path; tests skip when unset.
//! Never commit the image: personal vehicle data.
#[cfg(test)]
mod tests {
    fn load() -> Option<Vec<u8>> {
        let p = std::env::var("TIV_P01_BIN").ok()?;
        Some(std::fs::read(p).expect("TIV_P01_BIN set but unreadable"))
    }

    #[test]
    fn real_p01_identifies() {
        let Some(img) = load() else { return };
        assert_eq!(img.len(), 0x80000);
        assert_eq!(u32::from_be_bytes(img[0x504..0x508].try_into().unwrap()), 12225074);
        let id = crate::v29_tools::identify_bin(&img);
        assert_eq!(id["family"], "P01_0411", "{}", id["notes"]);
        assert_eq!(id["honda_os"], false);
        assert_eq!(crate::v29_tools::resolved_family(&img).unwrap(), "P01_0411");
    }

    #[test]
    fn real_p01_checksums_and_correction() {
        let Some(img) = load() else { return };
        let r = crate::checksum::validate_checksums(&img).unwrap();
        assert!(r.all_valid && r.valid_count == 8, "pristine image must validate: {:?}", r.regions);
        let mut dirty = img.clone();
        dirty[0x9000] = dirty[0x9000].wrapping_add(1);
        assert_eq!(crate::checksum::validate_checksums(&dirty).unwrap().failed_count, 1);
        let fixed = crate::checksum::correct_checksums(&dirty).unwrap();
        assert!(crate::checksum::validate_checksums(&fixed.data).unwrap().all_valid);
        let diffs: Vec<usize> = (0..img.len()).filter(|&i| fixed.data[i] != img[i]).collect();
        assert!(diffs.iter().all(|&i| matches!(i, 0x9000 | 0x8000 | 0x8001)), "{:X?}", diffs);
    }

    #[test]
    fn real_p01_cvns() {
        let Some(img) = load() else { return };
        let cvns = crate::checksum::p01_cvns(&img).unwrap();
        for c in &cvns { eprintln!("CVN {} seg{} pn{} ver{} cvn={} stock={:?}", c.name, c.segment_nr, c.part_number, c.version, c.cvn, c.stock_cvn); }
        assert_eq!(cvns[0].cvn, "717A");
        assert!(cvns.iter().all(|c| c.matches_stock != Some(false)), "pristine CVN mismatch vs stockcvn.xml");
    }

    #[test]
    fn real_p01_segments_and_slice() {
        let Some(img) = load() else { return };
        let segs = crate::checksum::p01_segments(&img).unwrap();
        assert_eq!(segs[1].cs_addr, 0x8000);
        assert!(segs.last().unwrap().blocks[0].1 < 0x20000);
        let seg = crate::ecu_database::flash_segment("P01_0411").unwrap();
        assert_eq!(seg.slice(&img).unwrap(), &img[0x8000..0x20000]);
    }

    #[test]
    fn real_p01_tables_read() {
        let Some(img) = load() else { return };
        let (t, located, _) = crate::tableseek::locate_p01_tables(&img);
        assert!(located > 1000);
        for d in t.iter().take(50) { let _ = crate::xdf::extract_table(&img, d); }
    }
}
