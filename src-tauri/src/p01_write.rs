//! P01 (0411, OS 12225074 / Intel 28F400B) calibration erase+write+CRC via the RAM kernel.
//!
//! Protocol bytes are taken from the public PcmHammer host source (GPL-3.0) used **as a
//! specification only** — this file is original MIT code, not a copy of that source.
//! Authoritative host source: github.com/PcmHammer/PcmHammer @ 033cb039 (fork of LegacyNsfw/PcmHacks).
//! Kernel busy reply: Kernels/68k-VPW-C/Kernel-P01.c HandleCrcQuery (7D FF).
//!   - Apps/PcmLibrary/Messages/Protocol.Kernel.cs  CreateFlashEraseBlockRequest / CreateCrcQuery / CreateExitKernel
//!   - Apps/PcmLibrary/Messages/Protocol.ReadWrite.cs  CreateBlockMessage / BlockCopyType.Copy
//!   - Apps/PcmLibrary/Misc/FlashChip.cs  case 0x00894471 (28F400B) cal = 0x8000/0x18000
//!   - Apps/PcmLibrary/Misc/Crc.cs  poly 0x04C11DB7 init 0 (file-side CRC for 3D02)
//!   - Apps/PcmLibrary/VpwKernelSession.cs  EraseRange / WriteRange
//!   - Apps/PcmLibrary/KernelVerifier.cs  CompareRanges (3D02 poll)
//!
//! Write payload size is fixed at 0x400 (same as the STN2255 kernel/read blocks in
//! reference/PcmHammer_debugLog_20260504@180810.txt). Device-dependent larger sizes are not guessed.
use crate::flash::BackupQuality;
use crate::p01_backup::{
    self, BackupProgress, VpwLink, FLASH_ID_28F400B, IMAGE_SIZE, KERNEL_P01, P01_KEY_ALGO, P01_OS_ID,
};
use serde::{Deserialize, Serialize};

/// FlashChip.cs case 0x00894471: Calibration 0x08000 len 0x18000.
pub const CAL_ADDR: u32 = 0x8000;
pub const CAL_LEN: u32 = 0x18000;
/// WriteRange payload size: match logged STN block size (0x400). Not DeviceMaxFlashWriteSendSize.
pub const WRITE_BLOCK: usize = 0x400;
/// BlockCopyType.Copy (Protocol.ReadWrite.cs).
pub const COPY_TYPE: u8 = 0x00;
pub const WRITE_RETRIES: usize = 3;
pub const CRC_POLL_MAX: usize = 64;

const H_6C: [u8; 3] = [0x6C, 0x10, 0xF0];
const H_6D: [u8; 3] = [0x6D, 0x10, 0xF0];
const R_6C: [u8; 3] = [0x6C, 0xF0, 0x10];
const R_6D: [u8; 3] = [0x6D, 0xF0, 0x10];

fn frame(h: [u8; 3], p: &[u8]) -> Vec<u8> { let mut v = h.to_vec(); v.extend_from_slice(p); v }

/// Protocol.Kernel.cs CreateFlashEraseBlockRequest: 3D 05 <addr24>.
pub fn build_erase(addr: u32) -> Vec<u8> {
    let a = addr.to_be_bytes();
    frame(H_6C, &[0x3D, 0x05, a[1], a[2], a[3]])
}
/// Protocol.Kernel.cs CreateCrcQuery: 3D 02 <size24> <addr24>.
pub fn build_crc_query(addr: u32, size: u32) -> Vec<u8> {
    let s = size.to_be_bytes(); let a = addr.to_be_bytes();
    frame(H_6C, &[0x3D, 0x02, s[1], s[2], s[3], a[1], a[2], a[3]])
}
/// Protocol.ReadWrite.cs CreateBlockMessage + VpwUtilities.AddBlockChecksum.
/// Header 6D 10 F0 36 <copyType> <len16> <addr24> <data> <sum16>; sum is bytes after mode.
pub fn build_write_block(addr: u32, data: &[u8]) -> Vec<u8> {
    p01_backup::build_kernel_block(COPY_TYPE, addr, data) // same framing; copyType 0x00
}
/// File-side CRC (Crc.cs): poly 0x04C11DB7, init 0, no reflect, no xorout.
pub fn pcmhammer_crc32(buf: &[u8], start: usize, len: usize) -> u32 {
    static TABLE: once_cell_lazy::Lazy<[u32; 256]> = once_cell_lazy::Lazy::new(|| {
        let mut t = [0u32; 256];
        for d in 0..256u32 {
            let mut r = d << 24;
            for _ in 0..8 { r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04C1_1DB7 } else { r << 1 }; }
            t[d as usize] = r;
        }
        t
    });
    let mut rem = 0u32;
    for i in start..start + len {
        let data = buf[i] ^ (rem >> 24) as u8;
        rem = TABLE[data as usize] ^ (rem << 8);
    }
    rem
}

// Avoid new dep: tiny once helper.
mod once_cell_lazy {
    use std::sync::OnceLock;
    pub struct Lazy<T, F = fn() -> T> { cell: OnceLock<T>, init: F }
    impl<T> Lazy<T> {
        pub const fn new(init: fn() -> T) -> Self { Self { cell: OnceLock::new(), init } }
    }
    impl<T> std::ops::Deref for Lazy<T> {
        type Target = T;
        fn deref(&self) -> &T { self.cell.get_or_init(self.init) }
    }
}

fn expect(r: Option<Vec<u8>>, want: &[u8], what: &str) -> Result<Vec<u8>, String> {
    let r = r.ok_or_else(|| format!("{}: no reply", what))?;
    if r.len() < want.len() || &r[..want.len()] != want { return Err(format!("{}: unexpected {:02X?}", what, r)); }
    Ok(r)
}
fn present(link: &mut dyn VpwLink) -> Result<(), String> {
    link.transact(&p01_backup::build_device_present(), false).map(|_| ())
}

/// Bring up the P01 read/write kernel (unlock + upload + flash-ID gate). Leaves kernel running.
/// Shared by backup and write. Cited: same sequence as p01_backup::read_full_image (log + Protocol.Kernel).
pub fn bring_up_kernel(link: &mut dyn VpwLink, logs: &mut Vec<String>) -> Result<u32, String> {
    if KERNEL_P01.len() != p01_backup::KERNEL_SIZE {
        return Err(format!("Kernel-P01.bin is {} bytes, expected {}", KERNEL_P01.len(), p01_backup::KERNEL_SIZE));
    }
    let r = expect(link.transact(&p01_backup::build_os_query(), true)?, &[0x6C, 0xF0, 0x10, 0x7C, 0x0A], "OS query")?;
    if r.len() != 9 { return Err("OS query: reply length".into()); }
    let os = u32::from_be_bytes([r[5], r[6], r[7], r[8]]);
    logs.push(format!("PCM OS {}", os));
    if os != P01_OS_ID {
        return Err(format!("PCM OS {} is not {}; write limited to OS {} / 28F400B.", os, P01_OS_ID, P01_OS_ID));
    }
    expect(link.transact(&p01_backup::build_disable_chatter(), true)?, &[0x6C, 0xF0, 0x10, 0x68, 0x00], "disable chatter")?;
    present(link)?;
    let r = expect(link.transact(&p01_backup::build_seed_request(), true)?, &[0x6C, 0xF0, 0x10, 0x67, 0x01], "seed")?;
    if r.len() != 7 { return Err("seed: reply length".into()); }
    let seed = u16::from_be_bytes([r[5], r[6]]);
    let key = crate::gm_keys::gm_2byte_key(P01_KEY_ALGO, seed);
    let r = expect(link.transact(&p01_backup::build_key(key), true)?, &[0x6C, 0xF0, 0x10, 0x67, 0x02], "key")?;
    if r.get(5) != Some(&0x34) { return Err(format!("unlock refused: {:02X?}", r)); }
    logs.push("Unlocked (67 02 34)".into());
    present(link)?;
    expect(link.transact(&p01_backup::build_kernel_upload_request(), true)?, &[0x6C, 0xF0, 0x10, 0x74, 0x00, 0x44], "kernel upload request")?;
    for (i, (sub, addr, off, len)) in p01_backup::kernel_plan().into_iter().enumerate() {
        present(link)?;
        let f = p01_backup::build_kernel_block(sub, addr, &KERNEL_P01[off..off + len]);
        expect(link.transact(&f, true)?, &[0x6D, 0xF0, 0x10, 0x76, 0x00, 0x73], &format!("kernel block {} @0x{:06X}", i, addr))?;
    }
    let mut ver = None;
    for _ in 0..2 { if let Some(r) = link.transact(&p01_backup::build_kernel_version(), true)? { ver = Some(r); break; } }
    let _ = expect(ver, &[0x6C, 0xF0, 0x10, 0x7D, 0x00], "kernel version")?;
    let r = expect(link.transact(&p01_backup::build_flash_id(), true)?, &[0x6C, 0xF0, 0x10, 0x7D, 0x01], "flash id")?;
    if r.len() != 9 { return Err("flash id: reply length".into()); }
    let id = u32::from_be_bytes([r[5], r[6], r[7], r[8]]);
    if id != FLASH_ID_28F400B {
        return Err(format!("flash chip 0x{:08X} is not Intel 28F400B (0x00894471). Refusing write.", id));
    }
    logs.push("Flash chip Intel 28F400B".into());
    Ok(os)
}

/// Parse erase reply: 6C F0 10 7D 05 <status>; status must be 0x00 (Protocol.Utility ParseByte / VpwKernelSession.EraseRange).
pub fn parse_erase_reply(r: &[u8]) -> Result<(), String> {
    if r.len() < 6 || r[..5] != [0x6C, 0xF0, 0x10, 0x7D, 0x05] {
        return Err(format!("erase: unexpected {:02X?}", r));
    }
    if r[5] != 0x00 { return Err(format!("erase failed, code {:02X}", r[5])); }
    Ok(())
}

/// Parse CRC reply: 6C F0 10 7D 02 <size24> <addr24> <crc32> (Protocol.Kernel ParseCrc).
/// Returns None if the kernel is still busy (`7D FF`, Kernel-P01.c HandleCrcQuery) so the caller can poll.
pub fn parse_crc_reply(r: &[u8], addr: u32, size: u32) -> Result<Option<u32>, String> {
    if r.len() >= 5 && r[..5] == [0x6C, 0xF0, 0x10, 0x7D, 0xFF] { return Ok(None); }
    let s = size.to_be_bytes(); let a = addr.to_be_bytes();
    let want = [0x6C, 0xF0, 0x10, 0x7D, 0x02, s[1], s[2], s[3], a[1], a[2], a[3]];
    if r.len() < 15 || r[..11] != want {
        if r.len() >= 6 && r[..4] == [0x6C, 0xF0, 0x10, 0x7F] { return Err("CRC query refused".into()); }
        return Err(format!("CRC: unexpected {:02X?}", r));
    }
    Ok(Some(u32::from_be_bytes([r[11], r[12], r[13], r[14]])))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub success: bool,
    pub bytes_written: u32,
    pub cal_crc_file: u32,
    pub cal_crc_pcm: Option<u32>,
    pub recovery_needed: bool,
    pub notes: String,
}

/// Erase + write + CRC-verify the calibration segment. Kernel must already be running
/// (call `bring_up_kernel` first). Always attempts halt afterwards.
pub fn write_calibration(
    link: &mut dyn VpwLink,
    image: &[u8],
    mut on_progress: impl FnMut(BackupProgress),
    logs: &mut Vec<String>,
) -> Result<WriteResult, String> {
    if image.len() != IMAGE_SIZE {
        return Err(format!("image is {} bytes; need {}", image.len(), IMAGE_SIZE));
    }
    let a = p01_backup::assess_image(image, P01_OS_ID);
    if a.quality != BackupQuality::FullImage {
        return Err(format!("tuned image is not FullImage-quality: {}", a.reasons.join("; ")));
    }
    let file_crc = pcmhammer_crc32(image, CAL_ADDR as usize, CAL_LEN as usize);
    logs.push(format!("File CRC cal 0x{:06X}+0x{:X} = {:08X}", CAL_ADDR, CAL_LEN, file_crc));

    // Init CRC engine on PCM (log after read-complete: 3D02000000000000).
    let _ = link.transact(&build_crc_query(0, 0), true)?;

    // Timeouts from PcmHammer ScanToolDeviceImplementation.GetTimeout (standard VPW).
    link.set_timeout_ms(7000)?; // EraseMemoryBlock
    present(link)?;
    let er = expect(link.transact(&build_erase(CAL_ADDR), true)?, &[0x6C, 0xF0, 0x10, 0x7D, 0x05], "erase")?;
    parse_erase_reply(&er)?;
    logs.push(format!("Erased calibration 0x{:06X}+0x{:X}", CAL_ADDR, CAL_LEN));

    link.set_timeout_ms(250)?; // WriteMemoryBlock
    let mut written = 0u32;
    let mut mid_fail: Option<String> = None;
    for off in (0..CAL_LEN as usize).step_by(WRITE_BLOCK) {
        let n = (CAL_LEN as usize - off).min(WRITE_BLOCK);
        let addr = CAL_ADDR + off as u32;
        let chunk = &image[addr as usize..addr as usize + n];
        let f = build_write_block(addr, chunk);
        let mut ok = false;
        let mut last = String::new();
        for _ in 0..WRITE_RETRIES {
            present(link)?;
            match link.transact(&f, true) {
                Ok(Some(r)) if r.len() >= 4 && r[..4] == [0x6D, 0xF0, 0x10, 0x76] => { ok = true; break; }
                Ok(Some(r)) => last = format!("write 0x{:06X}: unexpected {:02X?}", addr, r),
                Ok(None) => last = format!("write 0x{:06X}: no reply", addr),
                Err(e) => last = e,
            }
        }
        if !ok { mid_fail = Some(last); break; }
        written += n as u32;
        on_progress(BackupProgress {
            bytes_done: written, bytes_total: CAL_LEN, percent: (written as u64 * 100 / CAL_LEN as u64) as u8, stage: "write".into(),
        });
    }

    if let Some(e) = mid_fail {
        logs.push(format!("Write failed after {} bytes: {}", written, e));
        // VpwKernelSession.AfterFailedWrite: erase cal to force recovery.
        let _ = link.set_timeout_ms(7000);
        let _ = link.transact(&build_erase(CAL_ADDR), true);
        let _ = link.transact(&p01_backup::build_halt(), true);
        return Ok(WriteResult {
            success: false, bytes_written: written, cal_crc_file: file_crc, cal_crc_pcm: None,
            recovery_needed: true, notes: e,
        });
    }

    // CRC verify (KernelVerifier.CompareRanges). Busy reply 7D FF is from Kernel-P01.c HandleCrcQuery.
    link.set_timeout_ms(4000)?; // ReadCrc
    let mut pcm_crc = None;
    for _ in 0..CRC_POLL_MAX {
        present(link)?;
        match link.transact(&build_crc_query(CAL_ADDR, CAL_LEN), true)? {
            Some(r) => match parse_crc_reply(&r, CAL_ADDR, CAL_LEN)? {
                Some(c) => { pcm_crc = Some(c); break; }
                None => continue, // 7D FF busy
            },
            None => continue,
        }
    }

    match pcm_crc {
        Some(c) if c == file_crc => {
            let _ = link.transact(&p01_backup::build_halt(), true);
            logs.push(format!("CRC verify OK {:08X}", c));
            Ok(WriteResult {
                success: true, bytes_written: written, cal_crc_file: file_crc, cal_crc_pcm: Some(c),
                recovery_needed: false, notes: "calibration write verified".into(),
            })
        }
        Some(c) => {
            logs.push(format!("CRC mismatch file {:08X} pcm {:08X}; erasing cal for recovery", file_crc, c));
            let _ = link.set_timeout_ms(7000);
            let _ = link.transact(&build_erase(CAL_ADDR), true);
            let _ = link.transact(&p01_backup::build_halt(), true);
            Ok(WriteResult {
                success: false, bytes_written: written, cal_crc_file: file_crc, cal_crc_pcm: Some(c),
                recovery_needed: true,
                notes: format!("CRC mismatch file {:08X} pcm {:08X}. Cal erased for recovery; re-flash from FullImage backup.", file_crc, c),
            })
        }
        None => {
            let _ = link.transact(&p01_backup::build_halt(), true);
            Err("CRC verify: no reply from kernel".into())
        }
    }
}

/// Full path: bring up kernel, write cal, halt. For orchestrate_guided_flash / tests.
pub fn write_full(
    link: &mut dyn VpwLink,
    image: &[u8],
    on_progress: impl FnMut(BackupProgress),
    logs: &mut Vec<String>,
) -> Result<WriteResult, String> {
    let os = bring_up_kernel(link, logs)?;
    if os != P01_OS_ID { return Err(format!("OS mismatch {}", os)); }
    write_calibration(link, image, on_progress, logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p01_backup::tests::synth;

    /// Scripted kernel that accepts erase/write/CRC for the cal segment.
    struct FakeWritePcm {
        image: Vec<u8>,
        flash: Vec<u8>,
        unlocked: bool,
        running: bool,
        erased: bool,
        writes: u32,
        halt: bool,
        crc_busy: u8,
    }
    impl FakeWritePcm {
        fn new(image: Vec<u8>) -> Self {
            Self { flash: image.clone(), image, unlocked: false, running: false, erased: false, writes: 0, halt: false, crc_busy: 2 }
        }
    }
    impl VpwLink for FakeWritePcm {
        fn transact(&mut self, f: &[u8], _e: bool) -> Result<Option<Vec<u8>>, String> {
            let r6c = |p: &[u8]| Some(frame(R_6C, p));
            Ok(match (&f[..3], &f[3..]) {
                ([0x8C, 0xFE, 0xF0], [0x3F]) => None,
                ([0x6C, 0x10, 0xF0], [0x3C, 0x0A]) => { let o = P01_OS_ID.to_be_bytes(); r6c(&[0x7C, 0x0A, o[0], o[1], o[2], o[3]]) }
                ([0x6C, 0xFE, 0xF0], [0x28, 0x00]) => r6c(&[0x68, 0x00]),
                ([0x6C, 0x10, 0xF0], [0x27, 0x01]) => r6c(&[0x67, 0x01, 0x16, 0xCB]),
                ([0x6C, 0x10, 0xF0], [0x27, 0x02, 0xC8, 0x37]) => { self.unlocked = true; r6c(&[0x67, 0x02, 0x34]) }
                ([0x6C, 0x10, 0xF0], [0x34, 0x00, 0x10, 0x00, 0xFF, 0x80, 0x00]) if self.unlocked => r6c(&[0x74, 0x00, 0x44]),
                ([0x6D, 0x10, 0xF0], [0x36, rest @ ..]) => {
                    let n = u16::from_be_bytes([rest[1], rest[2]]) as usize;
                    let a = u32::from_be_bytes([0, rest[3], rest[4], rest[5]]);
                    if rest[0] == 0x80 { self.running = true; }
                    else if self.running && a >= CAL_ADDR && a < CAL_ADDR + CAL_LEN {
                        self.flash[a as usize..a as usize + n].copy_from_slice(&rest[6..6 + n]);
                        self.writes += 1;
                    }
                    Some(frame(R_6D, &[0x76, 0x00, 0x73]))
                }
                ([0x6C, 0x10, 0xF0], [0x3D, 0x00]) if self.running => r6c(&[0x7D, 0x00, 0x01, 0x03, 0x05, 0x01]),
                ([0x6C, 0x10, 0xF0], [0x3D, 0x01]) if self.running => {
                    let i = FLASH_ID_28F400B.to_be_bytes(); r6c(&[0x7D, 0x01, i[0], i[1], i[2], i[3]])
                }
                ([0x6C, 0x10, 0xF0], [0x3D, 0x05, a0, a1, a2]) if self.running => {
                    let a = u32::from_be_bytes([0, *a0, *a1, *a2]);
                    assert_eq!(a, CAL_ADDR);
                    for b in &mut self.flash[CAL_ADDR as usize..(CAL_ADDR + CAL_LEN) as usize] { *b = 0xFF; }
                    self.erased = true;
                    r6c(&[0x7D, 0x05, 0x00])
                }
                ([0x6C, 0x10, 0xF0], [0x3D, 0x02, s0, s1, s2, a0, a1, a2]) if self.running => {
                    let size = u32::from_be_bytes([0, *s0, *s1, *s2]);
                    let addr = u32::from_be_bytes([0, *a0, *a1, *a2]);
                    if size == 0 && addr == 0 { return Ok(r6c(&[0x7D, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0])); }
                    if self.crc_busy > 0 { self.crc_busy -= 1; return Ok(r6c(&[0x7D, 0xFF, 0x01])); }
                    let c = pcmhammer_crc32(&self.flash, addr as usize, size as usize).to_be_bytes();
                    r6c(&[0x7D, 0x02, *s0, *s1, *s2, *a0, *a1, *a2, c[0], c[1], c[2], c[3]])
                }
                ([0x6C, 0x10, 0xF0], [0x20]) => { self.halt = true; self.running = false; r6c(&[0x60]) }
                _ => r6c(&[0x7F]),
            })
        }
    }

    #[test]
    fn frames_match_pcmhammer_spec() {
        assert_eq!(build_erase(CAL_ADDR), vec![0x6C, 0x10, 0xF0, 0x3D, 0x05, 0x00, 0x80, 0x00]);
        assert_eq!(build_crc_query(CAL_ADDR, CAL_LEN), vec![0x6C, 0x10, 0xF0, 0x3D, 0x02, 0x01, 0x80, 0x00, 0x00, 0x80, 0x00]);
        assert_eq!(build_crc_query(0, 0x4000)[3..], [0x3D, 0x02, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00]); // log boot query
        let b = build_write_block(CAL_ADDR, &[0u8; 4]);
        assert_eq!(&b[..5], &[0x6D, 0x10, 0xF0, 0x36, COPY_TYPE]);
    }

    #[test]
    fn crc_matches_logged_boot_on_reference_bin() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../reference/LS1 PCM P01 12225074.bin");
        let d = std::fs::read(p).unwrap();
        assert_eq!(pcmhammer_crc32(&d, 0, 0x4000), 0xEDAB_67C6); // log line ~7858
    }

    #[test]
    fn fake_kernel_write_verifies() {
        let mut img = synth();
        // Flip a cal byte so write is non-trivial, then correct CS.
        img[0x9000] ^= 0x11;
        img = crate::checksum::correct_checksums(&img).unwrap().data;
        let mut pcm = FakeWritePcm::new(synth()); // start from stock in flash
        let mut logs = vec![];
        let r = write_full(&mut pcm, &img, |_| {}, &mut logs).unwrap();
        assert!(r.success, "{:?} {}", r, logs.join("; "));
        assert!(pcm.erased && pcm.halt);
        assert_eq!(pcm.flash[CAL_ADDR as usize..(CAL_ADDR + CAL_LEN) as usize],
                   img[CAL_ADDR as usize..(CAL_ADDR + CAL_LEN) as usize]);
        assert_eq!(r.cal_crc_pcm, Some(r.cal_crc_file));
    }

    #[test]
    fn wrong_chip_refused() {
        let img = synth();
        struct BadChip(FakeWritePcm);
        impl VpwLink for BadChip {
            fn transact(&mut self, f: &[u8], e: bool) -> Result<Option<Vec<u8>>, String> {
                if f.get(3..5) == Some(&[0x3D, 0x01]) {
                    return Ok(Some(vec![0x6C, 0xF0, 0x10, 0x7D, 0x01, 0x00, 0x89, 0x44, 0x72]));
                }
                self.0.transact(f, e)
            }
        }
        let mut pcm = BadChip(FakeWritePcm::new(img.clone()));
        assert!(write_full(&mut pcm, &img, |_| {}, &mut vec![]).unwrap_err().contains("28F400B"));
    }
}
