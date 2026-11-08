# Office viewer

rCommander's bundled viewer plugin for Word, Excel and PowerPoint documents in Office Open XML
(RC-100-99): `.docx`, `.docm`, `.xlsx`, `.xlsm`, `.pptx` and `.pptm`. Its `probe` takes a zip
whose central directory has `[Content_Types].xml`, so another zip named `.docx` goes to the next
viewer. It offers two views of one document, switched like the viewer's own modes with **7**:

- **text** (shown first) - what the document says, as plain UTF-8:
  - a **Word** document: its paragraphs in order, a line each; a table row as one line of its
    cells separated by tabs, the paragraphs of one cell joined by a space;
  - a **workbook**: sheet by sheet in the order of `xl/workbook.xml`, the sheet's name on a line
    of its own, then its rows as tab-separated values - shared strings, inline strings,
    `TRUE`/`FALSE`, and for a formula the result Excel last saved, never the formula. A cell keeps
    its column, so an empty cell is an empty field. The first row is the header; the setting
    `sheet_rows` rows below it are shown and the rest counted in one line,
    `[... 500 more rows]`. Numbers are shown as stored: a date is its serial number;
  - a **presentation**: slide by slide in the order of `ppt/presentation.xml` (not of the part
    names), each under a `Slide n` line, its text frames and tables as in Word.
- **tree** - the document properties (`title`, `subject`, `creator`, `last modified by`,
  `created`, `modified`, `application`, `pages`, `words`), then for a workbook a `sheets` node with
  each sheet's rows and columns, for a presentation a `slides` node with each slide's title.

## Settings

On the plugin's page under Settings -> Plugins, read when a file is opened:

| Setting | Values | Default |
| --- | --- | --- |
| `sheet_rows` | `1000`, `10000`, `100000` | `10000` - the rows shown per sheet below its header |

## What it does not do

- **No macros.** No part named `*.bin` is ever opened - `vbaProject.bin` and embedded objects
  among them - whatever the relationships say.
- **Nothing outside the package.** Every part is found through the relationships (`_rels/*.rels`),
  never by a fixed path alone; a relationship with `TargetMode="External"` (a link, a linked
  picture, an attached template) is dropped, and a target that names a scheme or climbs out of
  the package is never opened. It needs no capability, and the sandbox has neither file system
  nor network.
- **No pictures, no layout.** Images, charts, comments, headers, footers and footnotes are not
  shown; neither are OpenDocument files (`.odt`, `.ods`, `.odp`) or the binary formats (`.doc`,
  `.xls`, `.ppt`). A password-protected document is not a zip and goes to the next viewer.

## Limits

Each refusal is `bad-data` with its own message (`locales/`):

- at most 10,000 entries in the zip, counted on its end record before the central directory is
  read (every end record in the last 64 KiB, with its Zip64 record) - `too_many_entries`;
- each part at most 64 MiB unpacked: the declared size is checked before the part is opened, and
  the bytes actually inflated are counted, one byte past the cap at most - `too_large`;
- each part at most 500 times its packed size, as rCommander's own archive limits, checked once
  it has inflated 1 MiB - `too_compressed`;
- XML at most 256 elements deep - `too_deep`; no entity is expanded beyond the five XML defines,
  so a DOCTYPE cannot grow a part;
- a central directory over 16 MiB, a part that is encrypted, packed other than stored or
  Deflate, longer than its directory entry says, failing its CRC-32 or not well-formed XML -
  `damaged`;
- the text at most 256 MiB (the viewer's own output limit cuts at its setting anyway).

What is left ends at the host's limits (`manifest.toml`: 256 MiB of memory, 60 s, ten times the
default fuel) as a notice. A file of up to 1 GiB is offered to the plugin: only the central
directory and the XML parts a view needs are read, never a picture or a video.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `children`) and `probe`, with the host's promises at
  the top; the main part found through `_rels/.rels`, its kind by its root element
- `src/limits.rs` - the limits and the failure codes
- `src/source_io.rs`, `src/entries.rs`, `src/central.rs`, `src/package.rs` - the plugin's own
  zip reader: the file as `Read + Seek` in windows, the end record (Zip64 included) with the
  entries counted on it, the central directory (at most 16 MiB), the parts stored or Deflate
  (flate2 on miniz_oxide), capped while they inflate and checked on length and CRC-32
- `src/xml.rs`, `src/rels.rs`, `src/props.rs` - the XML events, the relationships and their
  targets, the document properties
- `src/flow.rs`, `src/sheet.rs`, `src/slides.rs`, `src/text.rs`, `src/out.rs` - the text of Word
  and DrawingML, of sheets, of slides, written in windows under the cap
- `src/tree.rs` - the tree
- `src/settings.rs` - `sheet_rows`
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the setting; the fields
  every bundled plugin shares are in `../metadata.toml`
- `locales/` - the plugin's name, description, setting label and failure codes in English,
  German, French and Spanish
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.4.0`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-office-viewer
scripts/build-plugins.sh office-viewer        # component, signed package
```

The tests run natively, without a WebAssembly toolchain, on small packages they write with the
`zip` crate. `scripts/build-plugins.sh` builds the component for `wasm32-unknown-unknown`, checks
its imports, packages it with the shared fields of `../metadata.toml`, signs it with the
development key and runs the conformance check; the Windows package carries the result in
`plugins/`, and rCommander installs it at its first start.

Licence: MIT.
