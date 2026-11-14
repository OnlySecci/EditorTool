//! Bounds are enforced on the opened file, before parsing or allocating decoded pixels.
use std::{io, path::Path};
pub const DOCUMENT: u64 = 64 * 1024 * 1024;
pub const IMAGE_BYTES: u64 = 32 * 1024 * 1024;
pub const PIXELS: u64 = 40_000_000;
pub const IMAGES: usize = 64;
pub fn read(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    crate::vault::read(path, path, max)
}
pub fn read_text(path: &Path) -> io::Result<String> {
    String::from_utf8(read(path, DOCUMENT)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
pub fn pixels(w: u32, h: u32) -> Result<u64, String> {
    let pixels = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || w > 32768 || h > 32768 || pixels > PIXELS {
        return Err("Image exceeds the limits (40 megapixels, 32768 pixels per side).".into());
    }
    Ok(pixels)
}
