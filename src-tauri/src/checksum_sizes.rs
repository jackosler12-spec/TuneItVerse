//! Shared P01 / EDC16 image sizes so checksum + flash agree.
pub const BLOCK_SIZE: usize = 0x10000;
pub const CAL_IMAGE_SIZE: usize = BLOCK_SIZE * 2;
pub const P01_FULL_IMAGE_SIZE: usize = BLOCK_SIZE * 8;
pub const EDC16_FLASH_SIZE: usize = 0x200000;

pub fn is_p01_size(len: usize) -> bool {
    len == CAL_IMAGE_SIZE || len == P01_FULL_IMAGE_SIZE
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
