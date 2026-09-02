//! Shared image sizes so checksum + identify + flash agree.
pub const BLOCK_SIZE: usize = 0x10000;
pub const CAL_IMAGE_SIZE: usize = BLOCK_SIZE * 2;
pub const P01_FULL_IMAGE_SIZE: usize = BLOCK_SIZE * 8;
pub const ME7_FLASH_SIZE: usize = 0x100000;
pub const EDC16_FLASH_SIZE: usize = 0x200000;

pub fn is_p01_size(len: usize) -> bool {
    len == CAL_IMAGE_SIZE || len == P01_FULL_IMAGE_SIZE
}

pub fn is_me7_size(len: usize) -> bool {
    len == ME7_FLASH_SIZE
}

pub fn p01_block_count(len: usize) -> Result<usize, String> {
    if !is_p01_size(len) {
        return Err(format!(
            "Expected {} or {} bytes for P01, got {}",
            CAL_IMAGE_SIZE, P01_FULL_IMAGE_SIZE, len
        ));
    }
    Ok(len / BLOCK_SIZE)
}
