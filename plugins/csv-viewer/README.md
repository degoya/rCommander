# CSV viewer

rCommander's bundled viewer plugin for `.csv`, `.tsv`, `.tab` and `.psv` files (RC-100-17). It
offers two views of one document, switched like the viewer's own modes:

- **highlighted** - the text with every column in its own colour, as "Rainbow CSV" shows it in an
  editor. The token classes are the contract's and language-neutral, so the columns take eight of
  them in turn (`key`, `string`, `number`, `keyword`, `type-name`, `function`, `literal`,
  `operator`; column 9 is `key` again) and the theme colours them. Separators are `punctuation`;
  a field's quotes are its column's colour; a broken field - text after its closing quote, or a
  quote never closed - is `invalid`. A quoted field over several lines keeps its column's colour
  on every line.
- **tree** - a node per record, `row <n>` counted from the first record after the header, valued
  with its first fields; unfolded, the fields, each labelled with its column's name from the header
  (`column <n>` without one). More than 1000 records, or fields of one record, unfold in ranges
  (`rows 1..1000`, ...).

It needs no capability: it reads the file it is handed and nothing else.

## Settings

On the plugin's page under Settings -> Plugins, read when a file is opened:

| Setting | Values | `auto` |
| --- | --- | --- |
| `separator` | `auto`, `comma`, `semicolon`, `tab`, `pipe` | a tab for `.tsv` and `.tab`, a pipe for `.psv`; else the one of the four found most often outside quotes on the first 32 lines, where one found equally often on every line goes first (a German Excel file's decimal commas do not outvote its semicolons) |
| `header` | `auto`, `yes`, `no` | the first record is the header when none of its fields is a number |
| `encoding` | `auto`, `utf-8`, `windows-1252` | UTF-8 when the whole file is valid UTF-8 (or starts with a byte-order mark), else Windows-1252, as Excel writes CSV on Windows |

Whatever the file is written in, the text rCommander gets is UTF-8, and the tokens count its
bytes. Windows-1252 comes from `rc-plugin-text`, the encodings the bundled plugins share.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the state the line starts in - inside a
quoted field, and which column - and a checkpoint every 64 records. Colouring 50 lines in the
middle of a 100 MiB file lexes from the nearest checkpoint, a page of the tree reads from the
nearest record checkpoint; neither reads the file again. A line is coloured up to 64 KiB and
4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`, `children`) and `probe`, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one, and the three settings
- `src/machine.rs` - CSV as RFC 4180 has it, one byte at a time; the open pass, the tokens and
  the tree all read through it
- `src/detect.rs`, `src/encoding.rs` - what `auto` decides; Windows-1252 and UTF-8 to UTF-8
- `src/scan.rs`, `src/lines.rs` - the open pass and its line and record indexes
- `src/tokens.rs`, `src/text.rs` - the tokens of the lines asked for, the text
- `src/walk.rs`, `src/tree.rs` - records read from a checkpoint, the tree
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the three settings; the
  fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name, its setting labels and its failure codes
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`, with the `settings` import), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-csv-viewer
cargo test -p rc-plugin-csv-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh csv-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
