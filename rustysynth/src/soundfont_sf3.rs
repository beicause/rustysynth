use std::io::{Cursor, ErrorKind};
#[cfg(not(target_family = "wasm"))]
use std::thread;

use symphonia_codec_vorbis::VorbisDecoder;
use symphonia_core::audio::SampleBuffer;
use symphonia_core::codecs::{Decoder, DecoderOptions};
use symphonia_core::errors::Error as SymphoniaError;
use symphonia_core::formats::{FormatOptions, FormatReader};
use symphonia_core::io::MediaSourceStream;
use symphonia_format_ogg::OggReader;

use crate::error::SoundFontError;
use crate::sample_header::SampleHeader;

/// Number of zero-valued guard frames appended after every decoded sample.
///
/// The SoundFont 2 specification requires at least 46 zero sample points after
/// each sample. They keep the oscillator's `data[index + 1]` interpolation
/// reads in bounds and stop one sample's tail from reading into the next.
const GUARD_FRAMES: usize = 46;

/// Upper bound on the number of threads used to decode the samples.
///
/// Decoding a General MIDI font is dominated by a thousand independent Ogg
/// Vorbis streams, so it parallelizes almost linearly. Beyond about eight
/// threads the wall clock stops improving while the CPU time grows (the decode
/// becomes memory bound), so the count is capped rather than following
/// `available_parallelism` all the way up.
///
/// WebAssembly targets have no threads to hand out, so they decode the samples
/// one after another and do not need the cap.
#[cfg(not(target_family = "wasm"))]
const MAX_DECODE_THREADS: usize = 8;

/// Decodes the concatenated Ogg Vorbis sample streams of a SoundFont3 into a
/// single shared 16-bit PCM buffer and rewrites each sample header to index it.
///
/// In a SoundFont3, `start`/`end` of a compressed sample are byte offsets into
/// the `smpl` sub-chunk delimiting that sample's Ogg Vorbis stream (`end` is
/// exclusive), while `start_loop`/`end_loop` are sample-frame offsets relative
/// to the beginning of the individual decompressed sample. After decoding, all
/// four fields are absolute indices into the returned PCM buffer, matching the
/// layout used by SoundFont2.
///
/// This must run before the instrument regions are built, since the regions
/// copy the sample offsets while they are created.
pub(crate) fn decode_vorbis_samples(
    smpl: &[u8],
    sample_headers: &mut [SampleHeader],
) -> Result<Vec<i16>, SoundFontError> {
    let decoded = decode_samples(smpl, sample_headers)?;

    // Every sample is followed by the guard frames, and the total length is
    // known now that all of them are decoded, so the pool is allocated once
    // instead of growing (and copying itself) a thousand times.
    let capacity = decoded
        .iter()
        .map(|samples| samples.len() + GUARD_FRAMES)
        .sum();
    let mut wave_data: Vec<i16> = Vec::with_capacity(capacity);

    for (header, samples) in sample_headers.iter_mut().zip(decoded) {
        let base = wave_data.len() as i32;
        let length = samples.len() as i32;

        wave_data.extend_from_slice(&samples);
        wave_data.resize(wave_data.len() + GUARD_FRAMES, 0);

        // Rewrite the header to index the decoded pool.
        header.start = base;
        header.end = base + length;
        header.start_loop += base;
        header.end_loop += base;
    }

    Ok(wave_data)
}

/// Decodes the stream of every sample, in header order.
///
/// WebAssembly targets have no threads, so they decode every sample on the
/// calling thread.
#[cfg(target_family = "wasm")]
fn decode_samples(
    smpl: &[u8],
    sample_headers: &[SampleHeader],
) -> Result<Vec<Vec<i16>>, SoundFontError> {
    decode_samples_on_one_thread(smpl, sample_headers)
}

/// Decodes the stream of every sample, in header order.
///
/// The streams are independent of each other (a sample's byte range is relative
/// to the compressed chunk, not to the decoded pool), so they are decoded on
/// several threads, each taking a contiguous run of headers. The runs are
/// collected back in header order, so the error returned is the one of the
/// first failing sample, exactly as a sequential decode would report it.
#[cfg(not(target_family = "wasm"))]
fn decode_samples(
    smpl: &[u8],
    sample_headers: &[SampleHeader],
) -> Result<Vec<Vec<i16>>, SoundFontError> {
    let count = sample_headers.len();

    let threads = thread::available_parallelism()
        .map_or(1, |parallelism| parallelism.get())
        .clamp(1, MAX_DECODE_THREADS)
        .min(count);

    if threads < 2 {
        return decode_samples_on_one_thread(smpl, sample_headers);
    }

    let per_thread = count.div_ceil(threads);

    thread::scope(|scope| {
        let handles: Vec<_> = sample_headers
            .chunks(per_thread)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|header| decode_sample(smpl, header))
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect();

        // The runs of headers are collected in order, so returning the first
        // failure hit while flattening them reports the same sample as a
        // sequential decode would. A run stops at its own first failure, so a
        // later failure in the same run is never reached.
        let mut samples = Vec::with_capacity(count);

        for handle in handles {
            let decoded = handle.join().expect("a sample decoding thread panicked");
            samples.extend(decoded?);
        }

        Ok(samples)
    })
}

/// Decodes the stream of every sample, one after another, on the calling thread.
fn decode_samples_on_one_thread(
    smpl: &[u8],
    sample_headers: &[SampleHeader],
) -> Result<Vec<Vec<i16>>, SoundFontError> {
    sample_headers
        .iter()
        .map(|header| decode_sample(smpl, header))
        .collect()
}

/// Decodes the Ogg Vorbis stream of one sample.
fn decode_sample(smpl: &[u8], header: &SampleHeader) -> Result<Vec<i16>, SoundFontError> {
    let start = header.start;
    let end = header.end;

    if start < 0 || end < start || end as usize > smpl.len() {
        return Err(SoundFontError::SampleDecompressionFailed(format!(
            "the compressed data range of sample '{}' is out of bounds ({start}..{end})",
            header.name
        )));
    }

    // The loop points of a SoundFont3 are offsets relative to the start of
    // the decompressed sample, so they must not be negative. The sanity
    // check in `SoundFont::sanity_check` only catches a negative offset on
    // the first sample (where it is not shifted by the pool base), so
    // reject them here for every sample before adding the base offset.
    // This mirrors the SoundFont2 path, which also rejects negative loop
    // points unconditionally.
    if header.start_loop < 0 || header.end_loop < 0 {
        return Err(SoundFontError::SampleDecompressionFailed(format!(
            "the loop points of sample '{}' are invalid ({}..{})",
            header.name, header.start_loop, header.end_loop
        )));
    }

    decode_stream(&smpl[start as usize..end as usize])
}

/// Decompresses one Ogg Vorbis stream into 16-bit PCM samples.
///
/// SoundFont samples are mono, so the interleaved read of a mono stream is the
/// sample itself.
fn decode_stream(stream: &[u8]) -> Result<Vec<i16>, SoundFontError> {
    // Gapless trimming makes the decoder drop the encoder padding of the last
    // Ogg page, which is what defines the length of a sample here: without it
    // every sample would carry up to a codec block of extra frames.
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };

    // `MediaSourceStream` takes a `'static` source, so the stream is copied out of
    // the compressed chunk first. The copy is a couple of megabytes for a whole
    // General MIDI font, far below the cost of decoding it.
    let source = MediaSourceStream::new(Box::new(Cursor::new(stream.to_vec())), Default::default());
    let mut reader = OggReader::try_new(source, &options).map_err(decompression_error)?;

    let track = reader.default_track().ok_or_else(|| {
        SoundFontError::SampleDecompressionFailed(String::from(
            "the sample contains no Ogg Vorbis stream",
        ))
    })?;

    let track_id = track.id;
    let params = track.codec_params.clone();

    match params.channels.map(|channels| channels.count()) {
        Some(1) => (),
        Some(count) => {
            return Err(SoundFontError::SampleDecompressionFailed(format!(
                "expected a mono sample, but found {count} channels"
            )));
        }
        None => {
            return Err(SoundFontError::SampleDecompressionFailed(String::from(
                "expected a mono sample, but found an unknown channel layout",
            )));
        }
    }

    let mut decoder =
        VorbisDecoder::try_new(&params, &DecoderOptions::default()).map_err(decompression_error)?;

    let mut samples: Vec<i16> = Vec::new();
    let mut buffer: Option<SampleBuffer<i16>> = None;

    loop {
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            // A sample is a single logical stream, so the end of the stream
            // (or, for a chained stream, the start of another one) ends it.
            Err(SymphoniaError::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(err) => return Err(decompression_error(err)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet).map_err(decompression_error)?;

        let buffer = buffer
            .get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, *decoded.spec()));

        buffer.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buffer.samples());
    }

    Ok(samples)
}

fn decompression_error(err: impl std::fmt::Display) -> SoundFontError {
    SoundFontError::SampleDecompressionFailed(err.to_string())
}
