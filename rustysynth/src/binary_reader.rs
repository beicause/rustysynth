use std::cmp;
use std::io;
use std::io::ErrorKind;
use std::io::Read;

use crate::four_cc::FourCC;

/// Size of the stack buffer used when streaming bulky chunks.
const CHUNK_BUFFER_SIZE: usize = 1 << 16;

#[non_exhaustive]
pub(crate) struct BinaryReader {}

impl BinaryReader {
    pub(crate) fn read_i8<R: Read>(reader: &mut R) -> Result<i8, io::Error> {
        let mut data: [u8; 1] = [0; 1];
        reader.read_exact(&mut data)?;
        Ok(i8::from_le_bytes(data))
    }

    pub(crate) fn read_u8<R: Read>(reader: &mut R) -> Result<u8, io::Error> {
        let mut data: [u8; 1] = [0; 1];
        reader.read_exact(&mut data)?;
        Ok(u8::from_le_bytes(data))
    }

    pub(crate) fn read_i16<R: Read>(reader: &mut R) -> Result<i16, io::Error> {
        let mut data: [u8; 2] = [0; 2];
        reader.read_exact(&mut data)?;
        Ok(i16::from_le_bytes(data))
    }

    pub(crate) fn read_u16<R: Read>(reader: &mut R) -> Result<u16, io::Error> {
        let mut data: [u8; 2] = [0; 2];
        reader.read_exact(&mut data)?;
        Ok(u16::from_le_bytes(data))
    }

    pub(crate) fn read_i32<R: Read>(reader: &mut R) -> Result<i32, io::Error> {
        let mut data: [u8; 4] = [0; 4];
        reader.read_exact(&mut data)?;
        Ok(i32::from_le_bytes(data))
    }

    pub(crate) fn read_u32<R: Read>(reader: &mut R) -> Result<u32, io::Error> {
        let mut data: [u8; 4] = [0; 4];
        reader.read_exact(&mut data)?;
        Ok(u32::from_le_bytes(data))
    }

    pub(crate) fn read_i16_big_endian<R: Read>(reader: &mut R) -> Result<i16, io::Error> {
        let mut data: [u8; 2] = [0; 2];
        reader.read_exact(&mut data)?;
        Ok(i16::from_be_bytes(data))
    }

    pub(crate) fn read_i32_big_endian<R: Read>(reader: &mut R) -> Result<i32, io::Error> {
        let mut data: [u8; 4] = [0; 4];
        reader.read_exact(&mut data)?;
        Ok(i32::from_be_bytes(data))
    }

    pub(crate) fn read_i32_variable_length<R: Read>(reader: &mut R) -> Result<i32, io::Error> {
        let mut acc: i32 = 0;
        let mut count: i32 = 0;

        loop {
            let value = BinaryReader::read_u8(reader)? as i32;
            acc = (acc << 7) | (value & 127);
            if (value & 128) == 0 {
                break;
            }
            count += 1;
            if count == 4 {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "the length of the value must be equal to or less than 4",
                ));
            }
        }

        Ok(acc)
    }

    pub(crate) fn read_four_cc<R: Read>(reader: &mut R) -> Result<FourCC, io::Error> {
        let mut data: [u8; 4] = [0; 4];
        reader.read_exact(&mut data)?;
        Ok(FourCC::from_bytes(data))
    }

    pub(crate) fn read_fixed_length_string<R: Read>(
        reader: &mut R,
        length: usize,
    ) -> Result<String, io::Error> {
        let mut data: Vec<u8> = BinaryReader::read_raw_data(reader, length)?;

        let mut actual_length: usize = 0;
        for value in &data {
            if *value == 0 {
                break;
            }
            actual_length += 1;
        }

        // Replace non-ASCII characters with '?'.
        // Tabs and returns are preserved.
        for value in &mut data[0..actual_length] {
            if !(9..=126).contains(value) {
                *value = 63; // '?'
            }
        }

        data.truncate(actual_length);

        // Every byte is a valid ASCII character now (9..=126 or '?'), so this cannot fail.
        Ok(String::from_utf8(data).unwrap())
    }

    pub(crate) fn discard_data<R: Read>(reader: &mut R, mut size: usize) -> Result<(), io::Error> {
        // Read into a small stack buffer instead of allocating a Vec of the full size
        // just to throw the bytes away.
        let mut data: [u8; 4096] = [0; 4096];

        while size > 0 {
            let chunk = cmp::min(size, data.len());
            reader.read_exact(&mut data[..chunk])?;
            size -= chunk;
        }

        Ok(())
    }

    /// Reads exactly `size` bytes and hands them to `sink` in chunks.
    ///
    /// The sample data of a SoundFont2 is little-endian 16-bit PCM, while a
    /// SoundFont3 stores Ogg Vorbis streams instead, so the sample data chunk
    /// is read as raw bytes and interpreted later. Reading raw bytes also
    /// keeps odd-sized chunks (legal in SoundFont3) safe.
    ///
    /// The declared size is not trusted for the allocation: a corrupt file can
    /// claim gigabytes and only provide a few bytes. The size is reserved up
    /// front only when the allocator can provide it, which keeps large sample
    /// banks readable in a single pass; otherwise the buffer grows with the
    /// data that actually arrives.
    pub(crate) fn read_raw_data<R: Read>(
        reader: &mut R,
        size: usize,
    ) -> Result<Vec<u8>, io::Error> {
        const CHUNK: usize = 4096;

        let mut data: Vec<u8> = Vec::new();

        if data.try_reserve_exact(size).is_ok() {
            data.resize(size, 0);
            reader.read_exact(&mut data)?;
            return Ok(data);
        }

        let mut buffer: [u8; CHUNK] = [0; CHUNK];
        let mut remaining = size;

        while remaining > 0 {
            let count = cmp::min(remaining, CHUNK);
            reader.read_exact(&mut buffer[..count])?;
            data.extend_from_slice(&buffer[..count]);
            remaining -= count;
        }

        Ok(data)
    }

    /// Bulky chunks are streamed through a stack buffer instead of being
    /// materialised as one allocation that is then copied again: the sample
    /// data of a General MIDI font is well over 100 MB.
    pub(crate) fn read_chunked<R: Read>(
        reader: &mut R,
        size: usize,
        mut sink: impl FnMut(&[u8]),
    ) -> Result<(), io::Error> {
        let mut buffer: [u8; CHUNK_BUFFER_SIZE] = [0; CHUNK_BUFFER_SIZE];
        let mut remaining = size;

        while remaining > 0 {
            let length = cmp::min(remaining, CHUNK_BUFFER_SIZE);
            reader.read_exact(&mut buffer[..length])?;
            sink(&buffer[..length]);
            remaining -= length;
        }

        Ok(())
    }

    /// Skips the pad byte that RIFF inserts after a chunk of an odd size.
    ///
    /// The pad byte is not included in the chunk size, so a reader that does
    /// not skip it reads the following chunk header one byte off and reports a
    /// bogus chunk ID.
    /// <https://github.com/sinshu/rustysynth/issues/56>
    pub(crate) fn skip_chunk_padding<R: Read>(
        reader: &mut R,
        size: usize,
        bytes_left_in_list: usize,
    ) -> Result<(), io::Error> {
        // A file that ends right after an odd-sized chunk simply has no pad.
        if !size.is_multiple_of(2) && bytes_left_in_list > 0 {
            BinaryReader::read_u8(reader)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    // A corrupt file can claim a chunk of gigabytes while providing a few
    // bytes. Trusting the declared size used to allocate it up front, which
    // aborts the process when the allocation fails (#57).
    #[test]
    fn test_read_raw_data_does_not_trust_the_declared_size() {
        let mut reader = Cursor::new(vec![0_u8; 8]);
        let result = BinaryReader::read_raw_data(&mut reader, usize::MAX / 2);

        assert!(matches!(
            result,
            Err(ref error) if error.kind() == ErrorKind::UnexpectedEof
        ));
    }

    #[test]
    fn test_read_raw_data_reads_the_requested_size() {
        let mut reader = Cursor::new(vec![1_u8, 2, 3, 4, 5, 6]);
        let data = BinaryReader::read_raw_data(&mut reader, 5).unwrap();

        assert_eq!(data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_skip_chunk_padding() {
        // An odd sized chunk is followed by a pad byte.
        let mut reader = Cursor::new(vec![0xAA, 0xBB]);
        BinaryReader::skip_chunk_padding(&mut reader, 3, 4).unwrap();
        assert_eq!(reader.position(), 1);

        // An even sized chunk has no padding.
        let mut reader = Cursor::new(vec![0xAA, 0xBB]);
        BinaryReader::skip_chunk_padding(&mut reader, 4, 4).unwrap();
        assert_eq!(reader.position(), 0);

        // The last chunk of a list has no padding even when it is odd sized.
        let mut reader = Cursor::new(vec![0xAA, 0xBB]);
        BinaryReader::skip_chunk_padding(&mut reader, 3, 0).unwrap();
        assert_eq!(reader.position(), 0);
    }
}
