use std::cmp;
use std::io::Read;

use crate::binary_reader::BinaryReader;
use crate::error::SoundFontError;
use crate::four_cc::FourCC;
use crate::read_counter::ReadCounter;
use crate::sample_header::SampleHeader;

/// The sample data of a SoundFont as it comes off the stream.
///
/// The two formats are told apart by the `OggS` magic, but a SoundFont2 chunk
/// is decoded as it is read: the sample data of a General MIDI font is over
/// 100 MB, so buffering the raw bytes and converting them afterwards would
/// copy it twice (and touch an equal amount of zeroed memory first).
pub(crate) enum RawSampleData {
    /// SoundFont2: raw little-endian 16-bit PCM, already widened to `i16`.
    Pcm(Vec<i16>),
    /// SoundFont3: concatenated Ogg Vorbis streams, still compressed.
    Compressed(Vec<u8>),
}

#[non_exhaustive]
pub(crate) struct SoundFontSampleData {
    pub bits_per_sample: i32,
    pub wave_data: Vec<i16>,
}

impl SoundFontSampleData {
    /// Reads the `sdta` LIST chunk and returns its `smpl` sub-chunk.
    ///
    /// The chunk is either little-endian 16-bit PCM (SoundFont2) or
    /// concatenated Ogg Vorbis streams (SoundFont3); it is turned into PCM by
    /// [`SoundFontSampleData::new`]. The 24-bit `sm24` sub-chunk, which only
    /// applies to SoundFont2, is discarded.
    pub(crate) fn read<R: Read>(reader: &mut R) -> Result<RawSampleData, SoundFontError> {
        let chunk_id = BinaryReader::read_four_cc(reader)?;
        if chunk_id != b"LIST" {
            return Err(SoundFontError::ListChunkNotFound);
        }

        let end = BinaryReader::read_u32(reader)? as usize;
        let reader = &mut ReadCounter::new(reader);

        let list_type = BinaryReader::read_four_cc(reader)?;
        if list_type != b"sdta" {
            return Err(SoundFontError::InvalidListChunkType {
                expected: FourCC::from_bytes(*b"sdta"),
                actual: list_type,
            });
        }

        let mut sample_data: Option<RawSampleData> = None;

        while reader.bytes_read() < end {
            let id = BinaryReader::read_four_cc(reader)?;
            let size = BinaryReader::read_u32(reader)? as usize;

            match id.as_bytes() {
                b"smpl" => {
                    if sample_data.is_some() {
                        return Err(SoundFontError::InvalidSampleData);
                    }

                    sample_data = Some(Self::read_smpl(reader, size)?)
                }
                b"sm24" => BinaryReader::discard_data(reader, size)?,
                _ => return Err(SoundFontError::ListContainsUnknownId(id)),
            }

            BinaryReader::skip_chunk_padding(
                reader,
                size,
                end.saturating_sub(reader.bytes_read()),
            )?;
        }

        sample_data.ok_or(SoundFontError::SampleDataNotFound)
    }

    /// Reads the `smpl` sub-chunk, decoding SoundFont2 PCM as it goes.
    ///
    /// A SoundFont2 chunk is widened to `i16` while it is being streamed, so
    /// neither the zero-initialised byte buffer nor the second full-size copy
    /// of a read-then-convert approach is ever allocated. A SoundFont3 chunk is
    /// kept as bytes, since it is decompressed sample by sample later.
    fn read_smpl<R: Read>(reader: &mut R, size: usize) -> Result<RawSampleData, SoundFontError> {
        // SoundFont3 compressed samples start with the "OggS" magic.
        let mut magic: [u8; 4] = [0; 4];
        let head = cmp::min(size, magic.len());
        reader.read_exact(&mut magic[..head])?;

        if head == magic.len() && magic == *b"OggS" {
            let mut data: Vec<u8> = BinaryReader::reserve(size);
            data.extend_from_slice(&magic);
            BinaryReader::read_chunked(reader, size - head, |bytes| data.extend_from_slice(bytes))?;
            return Ok(RawSampleData::Compressed(data));
        }

        let mut wave_data: Vec<i16> = BinaryReader::reserve(size / 2);
        // An odd-sized chunk is legal, at least for SoundFont3, so the trailing
        // byte of one read has to be paired with the head of the next one.
        let mut carry: Option<u8> = None;

        let mut push = |bytes: &[u8]| {
            let mut bytes = bytes;

            if let Some(low) = carry.take()
                && let Some((&high, rest)) = bytes.split_first()
            {
                wave_data.push(i16::from_le_bytes([low, high]));
                bytes = rest;
            }

            let (pairs, rest) = bytes.as_chunks::<2>();
            wave_data.extend(pairs.iter().map(|pair| i16::from_le_bytes(*pair)));
            carry = rest.first().copied();
        };

        push(&magic[..head]);
        BinaryReader::read_chunked(reader, size - head, &mut push)?;

        Ok(RawSampleData::Pcm(wave_data))
    }

    /// Turns the raw `smpl` bytes into 16-bit PCM sample data.
    ///
    /// SoundFont2 stores the samples as raw little-endian 16-bit PCM. A
    /// SoundFont3 instead stores each sample as an Ogg Vorbis stream and keeps
    /// the streams concatenated in the `smpl` chunk (identified by the `OggS`
    /// magic); those are decompressed here, which also rewrites `sample_headers`
    /// so that they reference the decoded data instead of the compressed bytes.
    pub(crate) fn new(
        sample_data: RawSampleData,
        sample_headers: &mut [SampleHeader],
    ) -> Result<Self, SoundFontError> {
        let wave_data = match sample_data {
            RawSampleData::Compressed(sample_data) => {
                #[cfg(not(feature = "sf3"))]
                {
                    let _ = (sample_data, sample_headers);
                    return Err(SoundFontError::UnsupportedSampleFormat);
                }

                #[cfg(feature = "sf3")]
                {
                    crate::soundfont_sf3::decode_vorbis_samples(&sample_data, sample_headers)?
                }
            }

            // SoundFont2 data is already decoded, at the right width.
            RawSampleData::Pcm(wave_data) => wave_data,
        };

        if wave_data.len() < 2 {
            return Err(SoundFontError::SampleDataNotFound);
        }

        Ok(Self {
            bits_per_sample: 16,
            wave_data,
        })
    }
}
