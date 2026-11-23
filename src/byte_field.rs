use crate::{MmapError, Result};

/// Checked UTF-8 operations for fixed-width, zero-padded record fields.
pub trait ByteArrayExt {
    /// Reads up to the first NUL or the end of the field.
    fn as_str(&self) -> std::result::Result<&str, std::str::Utf8Error>;
    /// Replaces the field and clears trailing bytes. Errors preserve its contents.
    fn set_str(&mut self, value: &str) -> Result<()>;
}
impl ByteArrayExt for [u8] {
    fn as_str(&self) -> std::result::Result<&str, std::str::Utf8Error> {
        let end = self.iter().position(|&b| b == 0).unwrap_or(self.len());
        std::str::from_utf8(&self[..end])
    }
    fn set_str(&mut self, value: &str) -> Result<()> {
        if value.len() > self.len() || value.as_bytes().contains(&0) {
            return Err(MmapError::InvalidConfiguration(
                "UTF-8 field exceeds capacity or contains NUL".into(),
            ));
        }
        self[..value.len()].copy_from_slice(value.as_bytes());
        self[value.len()..].fill(0);
        Ok(())
    }
}
