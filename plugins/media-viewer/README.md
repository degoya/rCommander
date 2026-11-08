# Media viewer

rCommander's bundled viewer plugin for audio files, video files and the photos whose EXIF the
built-in image view does not read (RC-100-99). It shows what a file says about itself; it plays
nothing. Three views of one document, switched like the viewer's own modes:

- **tree** (first) - sections of facts: `Audio` or `File`, `Tracks`, the tags, `Pictures`,
  `Camera`, `GPS` and every EXIF field by its image.
- **text** - the same facts as indented `label: value` lines, for copying and searching.
- **image** - the embedded cover, passed on unchanged, when there is one, it is PNG or JPEG and
  the setting `cover` is `on`.

| Files | Read by | Facts |
| --- | --- | --- |
| `mp3`, `flac`, `ogg`, `opus`, `m4a`, `wav`, `aiff`, `aif`, `ape`, `wv`, `aac` | lofty 0.25 | format, duration, bitrate, sample rate, channels, bit depth; every tag (title, artist, album, album artist, date, track, genre, comment first, then the rest); the pictures; for `m4a` the tracks' codecs |
| `mp4`, `m4v`, `mov`, `3gp` | our own ISO base media reader | container, duration; per track kind, codec, resolution or channels and sample rate, duration, language; iTunes and QuickTime tags (a phone's make, model, location); the cover |
| `mkv`, `webm` | our own Matroska reader | container, duration, title, applications; per track kind, codec, name, resolution or sample rate and channels, language |
| `tif`, `tiff`, `dng`, `cr2`, `nef`, `arw`, `heic`, `heif` | kamadak-exif 0.6 | make, model, lens, time taken, exposure, f-number, ISO, focal length, dimensions, orientation, GPS position in decimal degrees, then every field |

`probe` checks that the file starts as the format its extension names (an ID3 tag or an MPEG
frame, `fLaC`, `OggS`, `RIFF`/`WAVE`, `FORM`/`AIFF`, `MAC `, `wvpk`, an ADTS frame, `ftyp`, the
EBML magic, `II*\0`/`MM\0*`); a file that does not goes to the next viewer.

It needs no capability: it reads the file it is handed and nothing else.

## Settings

On the plugin's page under Settings -> Plugins, read when a file is opened:

| Setting | Values | Default |
| --- | --- | --- |
| `cover` | `on`, `off` | `on`: an embedded cover is offered as the `image` output (and not read at all with `off`) |

## Large and hostile files

Films are many GiB, so the manifest names no `max_size`, and nothing reads a file whole:

- Every reader fetches through `read-at` windows, with a **budget** of bytes in all: lofty
  32 MiB (a raw AAC stream larger than that, which lofty walks frame by frame, ends with
  `too_large`), the video readers 32 MiB, a TIFF its first 8 MiB, a HEIF 16 MiB. lofty allocates
  at most 8 MiB for one tag item; a cover is passed on up to 8 MiB.
- The ISO base media reader walks the top-level boxes header by header (`mdat` skipped by its
  size) to `moov`, before or after the media data, at most 64 MiB; inside it reads only the heads
  of the boxes it names. The Matroska reader skips clusters by their size and stops at one of
  unknown size.
- Every box and element size is checked against what is left of its parent and of the file, with
  `checked_add`; a varint is at most eight bytes, an element ID at most four; the walks are of fixed
  depth and count their boxes and elements. A size that does not fit - a box larger than the file,
  a 64-bit size of `u64::MAX`, a truncated box - ends in `bad-data`, never in an overflow or a
  large allocation.
- TIFF and raw files: kamadak-exif's own reader would read the whole file; the first 8 MiB are
  parsed instead, where camera files keep their IFDs. A field whose value lies beyond them is left
  out.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `image`, `children`), `probe`, the failure codes
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored
- `src/io.rs` - the buffered, seekable reader with its budget (`Read + Seek + BufRead`)
- `src/kind.rs` - the reader for an extension, and the signatures `probe` checks
- `src/audio.rs` - lofty
- `src/bmff.rs`, `src/bmff_items.rs` - ISO base media: boxes and tracks; tags and the cover
- `src/ebml.rs`, `src/matroska.rs` - EBML varints and elements; Matroska `Info` and `Tracks`
- `src/photo.rs` - kamadak-exif
- `src/outline.rs`, `src/units.rs` - the tree and its text; durations, codec names
- `src/settings.rs` - the setting `cover`
- `src/fixtures.rs`, `src/fixtures_video.rs`, `src/test_support.rs` - the test files, built byte
  by byte, and the test sources
- `manifest.toml`, `locales/` (`en`, `de`, `fr`, `es`), `CHANGES.md`
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`, only for
  `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-media-viewer
scripts/build-plugins.sh media-viewer      # component, signed package
```

Licence: MIT. lofty is MIT/Apache-2.0, kamadak-exif BSD-2-Clause.
