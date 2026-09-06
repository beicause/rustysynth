# Unreleased

- MIDI parsing now follows the SMF specification more closely:
  - SysEx/Meta events clear running status (previously a following data byte was misparsed).
  - Non-positive time divisions (incl. SMPTE) are rejected with `MidiFileError::InvalidTimeDivision`.
  - A zero-tempo change keeps the previous tempo instead of collapsing all subsequent timing.
  - `MidiFile::get_length` no longer panics on an empty message list.
- Exposed the merged events of a MIDI file for custom sequencers: `MidiMessage`
  is now a public type, and `MidiFile` provides `get_messages` and `get_times`.
- `MidiFile` can now also be built programmatically: `new_with_events`,
  `extend_events`, `clear` and `Default`. Both accept any iterator of
  `(time, message)` pairs, so no intermediate `Vec` needs to be allocated;
  `extend_events` appends in place and reuses the existing storage, so
  building a file incrementally only allocates when the new events no longer
  fit. Times must be in non-decreasing order, enforced via
  `MidiFileError::InvalidEventList` (an invalid batch leaves the file
  unchanged).

# v1.4.0

- Renamed the package to `rustysynth-ext` so this fork can be published to
  crates.io as a separate crate, distinct from the upstream `rustysynth`.
- Added optional SoundFont3 (`.sf3`) support behind the `sf3` cargo feature,
  using the pure-Rust `lewton` Ogg Vorbis decoder. With the feature enabled,
  `SoundFont::new` loads `.sf3` files transparently and decompresses every
  compressed sample into the same 16-bit PCM layout used by `.sf2` files.
- Fixed a panic when loading malformed SoundFonts whose zone lists contain
  negative or overflowing generator spans (now fails with
  `SoundFontError::InvalidZoneList` instead).
- Fixed negative loop points in `.sf3` samples being silently accepted on
  every sample but the first (now rejected, matching the `.sf2` path).
- Eliminated unnecessary `Vec` allocations in the sample-loading path for
  better performance.

# v1.3.6

- Various code clean-ups ([thanks to @sevonj](https://github.com/sinshu/rustysynth/issues/42)).
- Fixed segfault caused by empty `smpl` sub-chunks ([thanks to @sevonj](https://github.com/sinshu/rustysynth/pull/48)).
- Added sanity check for zero-length loops ([thanks to @eswartz](https://github.com/sinshu/rustysynth/pull/51)).

# v1.3.5

- Improved error reporting for invalid SoundFonts ([thanks to @sevonj](https://github.com/sinshu/rustysynth/pull/32)).
- Fixed issue where the loop mode was not handled correctly ([thanks to @sevonj](https://github.com/sinshu/rustysynth/pull/36)).
- All public types now implement the `Debug` trait.

# v1.3.4

- Some minor optimization.
- Fixed an issue where reading certain invalid SoundFonts results in a panic instead of an error ([thanks to @sevonj](https://github.com/sinshu/rustysynth/pull/31)).

# v1.3.3

- Some minor optimizations.
- Fixed an issue where the pitch bend range was incorrect in certain MIDI files ([thanks to @sevonj](https://github.com/sinshu/rustysynth/pull/29)).

# v1.3.2

- Added sanity check in loading SoundFonts ([thanks to @sevonj](https://github.com/sinshu/rustysynth/pull/24)).

# v1.3.1

- Now all the error types don't use heap allocation.

# v1.3.0

- Fixed issue where loading large SoundFont files would fail ([thanks to @paxbun](https://github.com/sinshu/rustysynth/pull/12)).
- Error types no longer allocate `String` ([thanks to @paxbun](https://github.com/sinshu/rustysynth/pull/12)).

# v1.2.1

- Minor tweaks to make the code idiomatic.
- Added `get_sample_id` method to `InstrumentRegion` ([thanks to @pomscyth](https://github.com/sinshu/rustysynth/pull/11)).
- Added `get_instrument_id` method to `PresetRegion`.

# v1.2.0

- Added ability to set the loop point when playing MIDI files.
- Added ability to change the playback speed on the fly when playing MIDI files.
- Added doc comments.

# v1.1.2

- Optimized chorus for better performance.

# v1.1.1

- Fixed issue where reading MIDI files with events inserted after EOT would fail ([thanks to @ArthurCose](https://github.com/sinshu/rustysynth/pull/9)).

# v1.1.0

- Error types are now `non_exhaustive`.
- Loading SoundFont3 explicitly fails with an error `SoundFontError::UnsupportedSampleFormat`.

# v1.0.0

- Introduced custom error types for error reporting.
- Removed unnecessary code.

# v0.9.2

- Refactored the entire code to be more idiomatic ([thanks to @joseluis](https://github.com/sinshu/rustysynth/pull/6)).
- Fixed issue where locks occurred during the rendering process.

# v0.9.1

- Modified the API to accommodate multi-threaded applications ([thanks to @sapir](https://github.com/sinshu/rustysynth/pull/5)).

# v0.9.0

- Implemented reverb and chorus.

# v0.1.0

- First release.
