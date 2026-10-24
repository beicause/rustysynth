use std::io;
use std::io::Read;

use crate::binary_reader::BinaryReader;

/// Represents the version of a SoundFont.
#[derive(Debug)]
#[non_exhaustive]
pub struct SoundFontVersion {
    pub(crate) major: i16,
    pub(crate) minor: i16,
}

impl SoundFontVersion {
    const SIZE: usize = 4;

    pub(crate) fn default() -> Self {
        Self { major: 0, minor: 0 }
    }

    /// Reads the two version numbers and skips whatever the chunk declares
    /// beyond them, so that the following chunk is read at the right offset.
    pub(crate) fn new<R: Read>(reader: &mut R, size: usize) -> Result<Self, io::Error> {
        let major = BinaryReader::read_i16(reader)?;
        let minor = BinaryReader::read_i16(reader)?;
        BinaryReader::discard_data(reader, size.saturating_sub(Self::SIZE))?;

        Ok(Self { major, minor })
    }

    /// Gets the major version.
    pub fn get_major(&self) -> i32 {
        self.major as i32
    }

    /// Gets the minor version.
    pub fn get_minor(&self) -> i32 {
        self.minor as i32
    }
}
