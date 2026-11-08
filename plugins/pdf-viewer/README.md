# PDF viewer

rCommander's bundled viewer plugin for `.pdf` files (RC-100-99), on hayro 0.9 (pure Rust,
MIT OR Apache-2.0). Two views of one document, switched like the viewer's own modes:

- **text** (first) - each page's text after a `Page n` line, a blank line between pages. A PDF
  places glyphs, not words or lines: the text is put together from the glyphs' positions
  (`src/layout.rs`) - a glyph lower or higher than the one before starts a line, a wide gap is a
  space. Glyphs without a Unicode value are left out. hayro calls its Unicode extraction
  experimental; columns and tables come out in the order the page draws them.
- **tree** - title, author, subject, keywords, creator, producer, created, modified, pages,
  version, encrypted and the first page's size (points and millimetres).

A document whose user password is empty opens as any other; one that needs a password is the
notice `password_required`, an encryption hayro cannot read `encryption`. Every page in one
picture (`pages`), the first page as a picture and the names of embedded files are not here.

It needs no capability: it reads the file it is handed and nothing else. No JavaScript, no link
and no embedded file is opened.

## Limits

- hayro parses a document in memory: `open` reads the file through `read-at` in 1 MiB windows,
  at most 64 MiB (`MAX_PDF_BYTES`, the manifest's `max_size`), asking `should-stop` before each.
- The text: at most 5000 pages (`[... n more pages]` after them), 4 MiB a page, 256 MiB in all;
  `should-stop` is asked before each page.
- `[limits]`: 256 MiB of memory (the document, hayro's objects, decoded streams and fonts), the
  fuel ceiling of 4e10 and 60 s.
- A document hayro cannot parse is `damaged`; a panic inside hayro on a hostile file ends the
  call as a trap, which the host counts towards the plugin's suspension.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `children`), `probe`, the failure codes, the limits
- `src/extract.rs` - a hayro `Device` that hands each glyph to the layout
- `src/layout.rs` - glyphs into lines and words
- `src/info.rs` - the information dictionary, version, encryption mark and page size
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`, `wasm32`
  only
- `manifest.toml`, `locales/` (en, de, fr, es), `CHANGES.md`

## Test and build

```bash
cargo test -p rc-plugin-pdf-viewer
scripts/build-plugins.sh pdf-viewer
```

The tests build small PDFs in memory (`src/test_support.rs`): two pages of Helvetica text, an
information dictionary, a document locked with a user password.

Licence: MIT. hayro's bundled standard fonts are PDFium's Foxit fonts (BSD-3-Clause).
