//! P01 (0411, OS 12225074) full 512KB read-back backup through PcmHammer's read kernel.
//!
//! Every frame is copied from reference/PcmHammer_debugLog_20260504@180810.txt
//! (PCM Hammer, STN2255, P01 OS 12225074); "log N" cites that file's line N.
//! Frames are 3-byte header + payload; the adapter adds/strips the J1850 CRC.
//! The STN STPX send framing follows reference/SerialDevice.cs SendMessage (lines 228-353).
use crate::flash::BackupQuality;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// reference/Kernel-P01.bin. Log 147 loads Kernel-P01.bin; log 157 "kernel size 7890, loadaddress FF8000".
pub const KERNEL_P01: &[u8] = include_bytes!("../../reference/Kernel-P01.bin");
pub const KERNEL_SIZE: usize = 7890;
pub const KERNEL_LOAD: u32 = 0xFF8000;
/// Log 164 end block "offset 0x1C00 ... length 0x2D2", then 0x400 blocks downward (log 179..233).
pub const KERNEL_BLOCK: usize = 0x400;
/// Log 281 "Reading from 0 / 0x0, length 1024" .. 6924 "Reading from 523264 / 0x7FC00".
pub const READ_BLOCK: usize = 0x400;
pub const IMAGE_SIZE: usize = 0x80000;
pub const READ_RETRIES: usize = 3;
pub const P01_OS_ID: u32 = 12225074;
/// Log 267-268 "7D 01 00 89 44 71" -> "Intel 28F400B, 512kb". Only chip ID established by the log.
pub const FLASH_ID_28F400B: u32 = 0x0089_4471;

const H_6C: [u8; 3] = [0x6C, 0x10, 0xF0];
const H_6D: [u8; 3] = [0x6D, 0x10, 0xF0];
const R_6C: [u8; 3] = [0x6C, 0xF0, 0x10];
const R_6D: [u8; 3] = [0x6D, 0xF0, 0x10];

fn frame(h: [u8; 3], p: &[u8]) -> Vec<u8> { let mut v = h.to_vec(); v.extend_from_slice(p); v }

/// 16-bit sum of every byte after the mode byte, big-endian. Checked against all 8
/// kernel 36 blocks (log 175..244) and all 512 read responses (log 283..6935).
pub fn block_sum(after_mode: &[u8]) -> u16 { after_mode.iter().fold(0u16, |a, &b| a.wrapping_add(b as u16)) }

/// Log 104-106: D:3C0A -> "6C F0 10 7C 0A 00 BA 8A 32", OSID 12225074.
pub fn build_os_query() -> Vec<u8> { frame(H_6C, &[0x3C, 0x0A]) }
/// Log 108-109: H:6CFEF0 D:2800 -> "6C F0 10 68 00".
pub fn build_disable_chatter() -> Vec<u8> { vec![0x6C, 0xFE, 0xF0, 0x28, 0x00] }
/// Log 114-115: H:8CFEF0 R:0 D:3F, no reply.
pub fn build_device_present() -> Vec<u8> { vec![0x8C, 0xFE, 0xF0, 0x3F] }
/// Log 121-122: D:2701 -> "67 01 16 CB".
pub fn build_seed_request() -> Vec<u8> { frame(H_6C, &[0x27, 0x01]) }
/// Log 125-127: D:2702C837 -> "67 02 34", "Unlock succeeded."
pub fn build_key(key: u16) -> Vec<u8> { let k = key.to_be_bytes(); frame(H_6C, &[0x27, 0x02, k[0], k[1]]) }
/// GM 2-byte algorithm 0x28 reproduces log 124 "(16CB, C837)".
pub const P01_KEY_ALGO: u16 = 0x28;
/// Log 158-159: D:34001000FF8000 -> "74 00 44". Copied byte-for-byte (not derived from 7890).
pub fn build_kernel_upload_request() -> Vec<u8> { frame(H_6C, &[0x34, 0x00, 0x10, 0x00, 0xFF, 0x80, 0x00]) }
/// Log 175/181/244: 36 <sub> <len16> <addr24> <data> <sum16>; sub 0x80 only on the
/// final block at 0xFF8000 (log 244, execute), 0x00 otherwise.
pub fn build_kernel_block(sub: u8, addr: u32, data: &[u8]) -> Vec<u8> {
    let n = (data.len() as u16).to_be_bytes();
    let a = addr.to_be_bytes();
    let mut p = vec![sub, n[0], n[1], a[1], a[2], a[3]];
    p.extend_from_slice(data);
    let s = block_sum(&p).to_be_bytes();
    p.extend_from_slice(&s);
    let mut v = H_6D.to_vec(); v.push(0x36); v.extend_from_slice(&p); v
}
/// Kernel blocks in the logged order: (sub, address, offset, length).
pub fn kernel_plan() -> Vec<(u8, u32, usize, usize)> {
    let mut out = vec![];
    let mut off = (KERNEL_SIZE / KERNEL_BLOCK) * KERNEL_BLOCK; // 0x1C00
    loop {
        let len = (KERNEL_SIZE - off).min(KERNEL_BLOCK);
        if len > 0 { out.push((if off == 0 { 0x80 } else { 0x00 }, KERNEL_LOAD + off as u32, off, len)); }
        if off == 0 { break; }
        off -= KERNEL_BLOCK;
    }
    out
}
/// Log 248-252: D:3D00 -> "7D 00 01 03 05 01" (first attempt timed out, log 249).
pub fn build_kernel_version() -> Vec<u8> { frame(H_6C, &[0x3D, 0x00]) }
/// Log 266-268: D:3D01 -> "7D 01 00 89 44 71".
pub fn build_flash_id() -> Vec<u8> { frame(H_6C, &[0x3D, 0x01]) }
/// Log 282: H:6D10F0 D:35010400000000 = 35 01 <len16> <addr24>.
pub fn build_read(addr: u32, len: u16) -> Vec<u8> {
    let n = len.to_be_bytes(); let a = addr.to_be_bytes();
    frame(H_6D, &[0x35, 0x01, n[0], n[1], a[1], a[2], a[3]])
}
/// Log 7846/7860-7861: D:20 -> "6C F0 10 60".
pub fn build_halt() -> Vec<u8> { frame(H_6C, &[0x20]) }

/// Parse a read reply (log 283): 6D F0 10 36 01 <len16> <addr24> <data> <sum16>.
pub fn parse_read_reply(r: &[u8], addr: u32, len: usize) -> Result<Vec<u8>, String> {
    if r.len() != 4 + 6 + len + 2 || r[..3] != R_6D || r[3] != 0x36 || r[4] != 0x01 {
        return Err(format!("read 0x{:05X}: unexpected reply ({} bytes)", addr, r.len()));
    }
    let n = u16::from_be_bytes([r[5], r[6]]) as usize;
    let a = u32::from_be_bytes([0, r[7], r[8], r[9]]);
    if n != len || a != addr { return Err(format!("read 0x{:05X}: reply is for 0x{:05X} len {}", addr, a, n)); }
    let body = &r[4..10 + len];
    let s = u16::from_be_bytes([r[10 + len], r[11 + len]]);
    if block_sum(body) != s { return Err(format!("read 0x{:05X}: block sum mismatch", addr)); }
    Ok(r[10..10 + len].to_vec())
}

/// One VPW request/response hop. `expect_reply = false` for broadcasts (R:0).
pub trait VpwLink {
    fn transact(&mut self, frame: &[u8], expect_reply: bool) -> Result<Option<Vec<u8>>, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupProgress { pub bytes_done: u32, pub bytes_total: u32, pub percent: u8, pub stage: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment { pub quality: BackupQuality, pub os_id: Option<u32>, pub reasons: Vec<String> }

/// FullImage only when: exactly 524288 bytes, OS ID @0x504 = expected, segment table
/// parses, and every P01 CS1 checksum passes (crate::checksum).
pub fn assess_image(img: &[u8], expected_os: u32) -> Assessment {
    let mut reasons = vec![];
    if img.len() != IMAGE_SIZE {
        return Assessment { quality: BackupQuality::Failed, os_id: None, reasons: vec![format!("image is {} bytes, need {}", img.len(), IMAGE_SIZE)] };
    }
    let os = u32::from_be_bytes([img[0x504], img[0x505], img[0x506], img[0x507]]);
    if os != expected_os { reasons.push(format!("OS ID @0x504 is {} but PCM reported {}", os, expected_os)); }
    match crate::checksum::p01_segments(img) {
        Err(e) => reasons.push(e),
        Ok(_) => match crate::checksum::validate_checksums(img) {
            Ok(r) if r.all_valid && r.regions.len() == 8 => {}
            Ok(r) => reasons.push(format!("P01 CS1: {} of {} segments failed", r.failed_count, r.regions.len())),
            Err(e) => reasons.push(e),
        },
    }
    let quality = if reasons.is_empty() { BackupQuality::FullImage } else { BackupQuality::Failed };
    Assessment { quality, os_id: Some(os), reasons }
}

fn expect(r: Option<Vec<u8>>, want: &[u8], what: &str) -> Result<Vec<u8>, String> {
    let r = r.ok_or_else(|| format!("{}: no reply", what))?;
    if r.len() < want.len() || &r[..want.len()] != want { return Err(format!("{}: unexpected reply {:02X?}", what, r)); }
    Ok(r)
}

fn present(link: &mut dyn VpwLink) -> Result<(), String> { link.transact(&build_device_present(), false).map(|_| ()) }

/// Run the full read. Returns the 512KB image and the OS ID the PCM reported.
/// Always tries to halt the kernel once it has been uploaded.
pub fn read_full_image(link: &mut dyn VpwLink, mut on_progress: impl FnMut(BackupProgress), logs: &mut Vec<String>) -> Result<(Vec<u8>, u32), String> {
    if KERNEL_P01.len() != KERNEL_SIZE { return Err(format!("Kernel-P01.bin is {} bytes, log 157 says {}", KERNEL_P01.len(), KERNEL_SIZE)); }
    let r = expect(link.transact(&build_os_query(), true)?, &[0x6C, 0xF0, 0x10, 0x7C, 0x0A], "OS query")?;
    if r.len() != 9 { return Err("OS query: reply length".into()); }
    let os = u32::from_be_bytes([r[5], r[6], r[7], r[8]]);
    logs.push(format!("PCM OS {}", os));
    if os != P01_OS_ID { return Err(format!("PCM OS {} is not {}; only OS {} read framing is established by the repo log. Refusing.", os, P01_OS_ID, P01_OS_ID)); }
    expect(link.transact(&build_disable_chatter(), true)?, &[0x6C, 0xF0, 0x10, 0x68, 0x00], "disable chatter")?;
    present(link)?;
    let r = expect(link.transact(&build_seed_request(), true)?, &[0x6C, 0xF0, 0x10, 0x67, 0x01], "seed")?;
    if r.len() != 7 { return Err("seed: reply length".into()); }
    let seed = u16::from_be_bytes([r[5], r[6]]);
    let key = crate::gm_keys::gm_2byte_key(P01_KEY_ALGO, seed);
    let r = expect(link.transact(&build_key(key), true)?, &[0x6C, 0xF0, 0x10, 0x67, 0x02], "key")?;
    if r.get(5) != Some(&0x34) { return Err(format!("unlock refused: {:02X?}", r)); }
    logs.push("Unlocked (67 02 34)".into());
    present(link)?;
    expect(link.transact(&build_kernel_upload_request(), true)?, &[0x6C, 0xF0, 0x10, 0x74, 0x00, 0x44], "kernel upload request")?;
    let res = (|| -> Result<Vec<u8>, String> {
        for (i, (sub, addr, off, len)) in kernel_plan().into_iter().enumerate() {
            present(link)?;
            let f = build_kernel_block(sub, addr, &KERNEL_P01[off..off + len]);
            expect(link.transact(&f, true)?, &[0x6D, 0xF0, 0x10, 0x76, 0x00, 0x73], &format!("kernel block {} @0x{:06X}", i, addr))?;
            on_progress(BackupProgress { bytes_done: 0, bytes_total: IMAGE_SIZE as u32, percent: 0, stage: format!("kernel {}/8", i + 1) });
        }
        let mut ver = None;
        for _ in 0..2 { if let Some(r) = link.transact(&build_kernel_version(), true)? { ver = Some(r); break; } }
        let v = expect(ver, &[0x6C, 0xF0, 0x10, 0x7D, 0x00], "kernel version")?;
        logs.push(format!("Kernel version {:02X?}", &v[5..]));
        let r = expect(link.transact(&build_flash_id(), true)?, &[0x6C, 0xF0, 0x10, 0x7D, 0x01], "flash id")?;
        if r.len() != 9 { return Err("flash id: reply length".into()); }
        let id = u32::from_be_bytes([r[5], r[6], r[7], r[8]]);
        if id != FLASH_ID_28F400B { return Err(format!("flash chip 0x{:08X} is not the logged Intel 28F400B. Refusing.", id)); }
        let mut img = Vec::with_capacity(IMAGE_SIZE);
        for addr in (0..IMAGE_SIZE).step_by(READ_BLOCK) {
            let mut last = String::new();
            let mut got = None;
            for _ in 0..READ_RETRIES {
                present(link)?;
                match link.transact(&build_read(addr as u32, READ_BLOCK as u16), true) {
                    Ok(Some(r)) => match parse_read_reply(&r, addr as u32, READ_BLOCK) { Ok(d) => { got = Some(d); break; } Err(e) => last = e },
                    Ok(None) => last = format!("read 0x{:05X}: no reply", addr),
                    Err(e) => last = e,
                }
            }
            img.extend_from_slice(&got.ok_or(last)?);
            let done = img.len() as u32;
            on_progress(BackupProgress { bytes_done: done, bytes_total: IMAGE_SIZE as u32, percent: (done as u64 * 100 / IMAGE_SIZE as u64) as u8, stage: "read".into() });
        }
        Ok(img)
    })();
    let _ = link.transact(&build_halt(), true); // log 7846 (no reply) / 7860-7861 ("60")
    res.map(|img| (img, os))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedBackup { pub path: String, pub sha256: String, pub os_id: Option<u32>, pub quality: BackupQuality, pub bytes: u32, pub reasons: Vec<String> }

pub fn sha256_hex(d: &[u8]) -> String { Sha256::digest(d).iter().map(|b| format!("{:02x}", b)).collect() }

/// Write `<dir>/p01_<os>_<unix ms>.bin` plus a `.json` sidecar with SHA-256 and quality.
/// A Failed read is still saved (for diagnosis) but marked Failed.
pub fn save_backup(dir: &std::path::Path, img: &[u8], a: &Assessment) -> Result<SavedBackup, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("backup dir: {}", e))?;
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let p = dir.join(format!("p01_{}_{}.bin", a.os_id.unwrap_or(0), ts));
    std::fs::write(&p, img).map_err(|e| format!("write backup: {}", e))?;
    let s = SavedBackup { path: p.to_string_lossy().into(), sha256: sha256_hex(img), os_id: a.os_id, quality: a.quality.clone(), bytes: img.len() as u32, reasons: a.reasons.clone() };
    std::fs::write(p.with_extension("json"), serde_json::to_vec_pretty(&s).unwrap()).map_err(|e| format!("write sidecar: {}", e))?;
    Ok(s)
}

/// Re-load a saved backup for the flash gate: sidecar hash must match the file and the
/// image must re-assess as FullImage. Anything else is an error (gate refuses).
pub fn load_verified_backup(path: &str) -> Result<SavedBackup, String> {
    let p = std::path::Path::new(path);
    let img = std::fs::read(p).map_err(|e| format!("backup {}: {}", path, e))?;
    let side: SavedBackup = serde_json::from_slice(&std::fs::read(p.with_extension("json")).map_err(|e| format!("backup sidecar: {}", e))?)
        .map_err(|e| format!("backup sidecar: {}", e))?;
    let h = sha256_hex(&img);
    if h != side.sha256 { return Err(format!("backup SHA-256 {} does not match sidecar {}. Refusing.", h, side.sha256)); }
    let os = side.os_id.ok_or("backup sidecar has no OS ID")?;
    let a = assess_image(&img, os);
    if a.quality != BackupQuality::FullImage { return Err(format!("backup is not FullImage: {}", a.reasons.join("; "))); }
    Ok(SavedBackup { quality: BackupQuality::FullImage, ..side })
}

/// Refuse a write whose OS ID differs from the backup's.
pub fn check_write_os(tuned: &[u8], backup: &SavedBackup) -> Result<(), String> {
    if tuned.len() != IMAGE_SIZE { return Err(format!("tuned image is {} bytes; OS check needs the full 512KB image", tuned.len())); }
    let t = u32::from_be_bytes([tuned[0x504], tuned[0x505], tuned[0x506], tuned[0x507]]);
    match backup.os_id {
        Some(b) if b == t => Ok(()),
        b => Err(format!("Tuned image OS {} differs from backup OS {:?}. Refusing write.", t, b)),
    }
}

/// STN (ScanTool) STPX link over the app's serial port. TX follows reference/SerialDevice.cs
/// SendMessage: "STPX H:hhhhhh, R:n, D:<hex>" under 200 bytes (lines 237, 270-297), else
/// "..., L:<payload len>" then the hex payload after a "DATA" reply (lines 319-343).
/// RX parsing (hex bytes, CR-terminated, '>' prompt) follows ElmDevice.cs ProcessResponse /
/// ElmDeviceImplementation.cs ReadELMLine. NOT bench-tested.
pub struct StnLink<'a> { pub port: &'a mut Box<dyn serialport::SerialPort + Send> }

impl<'a> StnLink<'a> {
    fn cmd(&mut self, s: &str) -> Result<String, String> {
        use std::io::{Read, Write};
        self.port.write_all(format!("{}\r", s).as_bytes()).map_err(|e| format!("STN write: {}", e))?;
        let mut out = String::new();
        let mut b = [0u8; 1];
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > std::time::Duration::from_secs(5) { return Err("STN: timeout waiting for prompt".into()); }
            match self.port.read(&mut b) {
                Ok(1) => { if b[0] == b'>' { break; } out.push(b[0] as char); if out.ends_with("DATA\r") { break; } }
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(format!("STN read: {}", e)),
            }
        }
        Ok(out)
    }
}

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{:02X}", x)).collect() }

impl<'a> VpwLink for StnLink<'a> {
    fn transact(&mut self, f: &[u8], expect_reply: bool) -> Result<Option<Vec<u8>>, String> {
        if f.len() < 4 { return Err("frame too short".into()); }
        let head = format!("STPX H:{}, R:{}", hex(&f[..3]), if expect_reply { 1 } else { 0 });
        let resp = if f.len() < 200 {
            self.cmd(&format!("{}, D:{}", head, hex(&f[3..])))?
        } else {
            let h = self.cmd(&format!("{}, L:{}", head, f.len() - 3))?;
            if !h.contains("DATA") { return Err(format!("STN: unexpected reply to STPX header: {}", h.trim())); }
            self.cmd(&hex(&f[3..]))?
        };
        if !expect_reply { return Ok(None); }
        for line in resp.split(['\r', '\n']) {
            let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if t.len() >= 8 && t.len() % 2 == 0 && t.chars().all(|c| c.is_ascii_hexdigit()) {
                let bytes: Vec<u8> = (0..t.len()).step_by(2).map(|i| u8::from_str_radix(&t[i..i + 2], 16).unwrap()).collect();
                if bytes[..3] == R_6C || bytes[..3] == R_6D { return Ok(Some(bytes)); }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted fake PCM: answers exactly the logged frames and serves `image`.
    struct FakePcm { image: Vec<u8>, os: u32, unlocked: bool, kernel: Vec<u8>, running: bool, drop_read_once: Option<u32>, flash_id: u32, halted: bool }
    impl FakePcm {
        fn new(image: Vec<u8>) -> Self { Self { image, os: P01_OS_ID, unlocked: false, kernel: vec![0; KERNEL_SIZE], running: false, drop_read_once: None, flash_id: FLASH_ID_28F400B, halted: false } }
    }
    impl VpwLink for FakePcm {
        fn transact(&mut self, f: &[u8], _e: bool) -> Result<Option<Vec<u8>>, String> {
            let r = |p: &[u8]| Some(frame(R_6C, p));
            Ok(match (&f[..3], &f[3..]) {
                ([0x8C, 0xFE, 0xF0], [0x3F]) => None,
                ([0x6C, 0x10, 0xF0], [0x3C, 0x0A]) => { let o = self.os.to_be_bytes(); r(&[0x7C, 0x0A, o[0], o[1], o[2], o[3]]) }
                ([0x6C, 0xFE, 0xF0], [0x28, 0x00]) => r(&[0x68, 0x00]),
                ([0x6C, 0x10, 0xF0], [0x27, 0x01]) => r(&[0x67, 0x01, 0x16, 0xCB]),
                ([0x6C, 0x10, 0xF0], [0x27, 0x02, a, b]) => { self.unlocked = [*a, *b] == [0xC8, 0x37]; if self.unlocked { r(&[0x67, 0x02, 0x34]) } else { r(&[0x7F, 0x27, 0x35]) } }
                ([0x6C, 0x10, 0xF0], [0x34, 0x00, 0x10, 0x00, 0xFF, 0x80, 0x00]) if self.unlocked => r(&[0x74, 0x00, 0x44]),
                ([0x6D, 0x10, 0xF0], [0x36, rest @ ..]) => {
                    let n = u16::from_be_bytes([rest[1], rest[2]]) as usize;
                    let a = u32::from_be_bytes([0, rest[3], rest[4], rest[5]]);
                    assert_eq!(block_sum(&rest[..6 + n]), u16::from_be_bytes([rest[6 + n], rest[7 + n]]));
                    let o = (a - KERNEL_LOAD) as usize; self.kernel[o..o + n].copy_from_slice(&rest[6..6 + n]);
                    if rest[0] == 0x80 { self.running = true; }
                    Some(frame(R_6D, &[0x76, 0x00, 0x73]))
                }
                ([0x6C, 0x10, 0xF0], [0x3D, 0x00]) if self.running => r(&[0x7D, 0x00, 0x01, 0x03, 0x05, 0x01]),
                ([0x6C, 0x10, 0xF0], [0x3D, 0x01]) if self.running => { let i = self.flash_id.to_be_bytes(); r(&[0x7D, 0x01, i[0], i[1], i[2], i[3]]) }
                ([0x6D, 0x10, 0xF0], [0x35, 0x01, n0, n1, a0, a1, a2]) if self.running => {
                    let n = u16::from_be_bytes([*n0, *n1]) as usize; let a = u32::from_be_bytes([0, *a0, *a1, *a2]);
                    if self.drop_read_once == Some(a) { self.drop_read_once = None; return Ok(None); }
                    let mut p = vec![0x01, *n0, *n1, *a0, *a1, *a2];
                    p.extend_from_slice(&self.image[a as usize..a as usize + n]);
                    let s = block_sum(&p).to_be_bytes(); p.extend_from_slice(&s);
                    let mut v = R_6D.to_vec(); v.push(0x36); v.extend_from_slice(&p); Some(v)
                }
                ([0x6C, 0x10, 0xF0], [0x20]) => { self.halted = true; self.running = false; r(&[0x60]) }
                _ => r(&[0x7F]),
            })
        }
    }

    pub(crate) fn synth() -> Vec<u8> {
        let mut d = crate::checksum::tests::synthetic_p01();
        d[0x504..0x508].copy_from_slice(&P01_OS_ID.to_be_bytes());
        crate::checksum::correct_checksums(&d).unwrap().data
    }

    #[test]
    fn frames_match_log() {
        assert_eq!(hex(&build_os_query()[3..]), "3C0A"); // log 104
        assert_eq!(hex(&build_key(crate::gm_keys::gm_2byte_key(P01_KEY_ALGO, 0x16CB))[3..]), "2702C837"); // log 125
        assert_eq!(hex(&build_kernel_upload_request()[3..]), "34001000FF8000"); // log 158
        assert_eq!(hex(&build_read(0, 0x400)[3..]), "35010400000000"); // log 282
        assert_eq!(hex(&build_read(0x7FC00, 0x400)[3..]), "3501040007FC00"); // log 6925
        let plan = kernel_plan();
        assert_eq!(plan.len(), 8);
        assert_eq!((plan[0].1, plan[0].3), (0xFF9C00, 0x2D2)); // log 164
        assert_eq!((plan[7].0, plan[7].1), (0x80, 0xFF8000)); // log 244
        assert_eq!(build_kernel_block(plan[0].0, plan[0].1, &KERNEL_P01[0x1C00..]).len() - 3, 731); // log 174 L:731
        let b = build_kernel_block(0x80, 0xFF8000, &KERNEL_P01[..0x400]);
        assert_eq!(hex(&b[b.len() - 2..]), "6FB0"); // log 244 ends 6FB0
        assert_eq!(KERNEL_P01.len(), KERNEL_SIZE);
    }

    #[test]
    fn full_read_from_fake_pcm_is_full_image() {
        let img = synth();
        let mut pcm = FakePcm::new(img.clone());
        pcm.drop_read_once = Some(0x40000);
        let mut logs = vec![]; let mut last = 0;
        let (got, os) = read_full_image(&mut pcm, |p| last = p.bytes_done, &mut logs).unwrap();
        assert_eq!(got, img); assert_eq!(os, P01_OS_ID); assert_eq!(last, IMAGE_SIZE as u32);
        assert_eq!(pcm.kernel, KERNEL_P01); assert!(pcm.halted);
        assert_eq!(assess_image(&got, os).quality, BackupQuality::FullImage);
        let dir = std::env::temp_dir().join(format!("tiv_p01_{}", std::process::id()));
        let s = save_backup(&dir, &got, &assess_image(&got, os)).unwrap();
        assert_eq!(s.sha256, sha256_hex(&img));
        let v = load_verified_backup(&s.path).unwrap();
        assert_eq!(v.quality, BackupQuality::FullImage);
        check_write_os(&img, &v).unwrap();
        let mut other = img.clone(); other[0x504..0x508].copy_from_slice(&12345678u32.to_be_bytes());
        assert!(check_write_os(&other, &v).is_err());
        let mut tampered = img.clone(); tampered[0x9000] ^= 1;
        std::fs::write(&s.path, &tampered).unwrap();
        assert!(load_verified_backup(&s.path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn refusals() {
        let img = synth();
        let mut logs = vec![];
        let mut p = FakePcm::new(img.clone()); p.os = 12345678;
        assert!(read_full_image(&mut p, |_| {}, &mut logs).is_err());
        let mut p = FakePcm::new(img.clone()); p.flash_id = 0x00894472;
        assert!(read_full_image(&mut p, |_| {}, &mut logs).is_err()); assert!(p.halted);
        let mut bad = img.clone(); bad[0x9000] ^= 0xFF;
        assert_eq!(assess_image(&bad, P01_OS_ID).quality, BackupQuality::Failed);
        assert_eq!(assess_image(&img, 12345678).quality, BackupQuality::Failed);
        assert_eq!(assess_image(&img[..0x7FC00], P01_OS_ID).quality, BackupQuality::Failed);
    }

    #[test]
    fn read_replies_from_log_parse() {
        let log = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../reference/PcmHammer_debugLog_20260504@180810.txt")).unwrap();
        let mut n = 0;
        for l in log.lines() {
            if let Some(i) = l.find("RX: 6D F0 10 36 ") {
                let b: Vec<u8> = l[i + 4..].split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect();
                let a = u32::from_be_bytes([0, b[7], b[8], b[9]]);
                parse_read_reply(&b, a, READ_BLOCK).unwrap(); n += 1;
            }
        }
        assert_eq!(n, 512);
    }

    /// Jack's real image, only when TIV_P01_BIN is set. Prints nothing from the image.
    #[test]
    fn real_image_when_env_set() {
        let Ok(p) = std::env::var("TIV_P01_BIN") else { return };
        let img = std::fs::read(p).unwrap();
        let mut pcm = FakePcm::new(img.clone());
        let (got, os) = read_full_image(&mut pcm, |_| {}, &mut vec![]).unwrap();
        assert!(got == img);
        assert_eq!(assess_image(&got, os).quality, BackupQuality::FullImage);
    }
}
