# JSON viewer

rCommander's bundled viewer plugin for `.json` and `.geojson` files (RC-100-11), and for
`.jsonc`, `.json5`, `.jsonl` and `.ndjson` (RC-100-80). It offers two views of one document,
switched like the viewer's own modes:

- **highlighted** - the text coloured like an editor: keys, strings, numbers, `true`/`false`/
  `null`, punctuation and anything invalid each in its theme colour. The setting `format` shows
  the text as it is in the file (`as_is`, the default) or indented two spaces a level
  (`pretty`), which turns a minified file into readable lines.
- **tree** - the structure to unfold: a key or `[index]` per node with its value, `{n}` or `[n]`
  for a container of `n` children. A container of more than 1000 children unfolds in ranges
  (`[0..999]`, ...). Invalid JSON shows a first node naming the line and column of the first
  error, and the tree lists what came before it.

It needs no capability: it reads the file it is handed and nothing else.

## Dialects

The file's extension picks how it is read (`src/dialect.rs`):

- `.json`, `.geojson` - JSON as RFC 8259 has it.
- `.jsonc` - JSON with `//` and `/* */` comments and a comma before a closing bracket.
- `.json5` - the same, keys without quotes, strings in single quotes, any character escaped,
  `0x` numbers, `+1`, `.5`, `5.`, `Infinity` and `NaN`. Not read: a string continued over a
  line break (a backslash at the line's end) - it is marked invalid from the break, as in JSON -
  and `Infinity` or `NaN` as a key without quotes.
- `.jsonl`, `.ndjson` - a value per line, each line checked for itself: a broken line is
  `invalid` from where it stops being a value, and only that line. The tree lists the lines'
  values as `[0]`, `[1]`, ... (blank lines left out); a broken line shows its error in its
  place. More than 16384 broken lines end the list at the next one.

Comments are coloured as comments; `pretty` lays out the values without them.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) and, for the large values, their end, their
child count and the offset of every 1000th child. Colouring 50 lines in the middle of a 100 MiB
file reads from the nearest checkpoint, a page of the tree from the nearest anchor; neither reads
the file again. A line is coloured up to 64 KiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`, `children`) and `probe`, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/dialect.rs` - JSON, JSONC, JSON5 or JSON Lines, by the file's extension
- `src/lexer.rs`, `src/lexer/comments.rs`, `src/tokens.rs` - the lexer, its comments and the
  tokens of one line
- `src/lines.rs`, `src/pretty.rs`, `src/text.rs` - the line index, the pretty printer, the text
- `src/structure.rs`, `src/walk.rs`, `src/tree.rs` - validation, the index of large values and
  the tree; `src/structure/records.rs`, `src/tree/records.rs` - the lines of JSON Lines
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the `format` setting;
  the fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and its failure codes
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`, only
  for `wasm32`

The component reads the setting `format` when a document opens (`settings.setting`, plugin
contract 0.2.0): `pretty` indents, `as_is` - the default - shows the file as it is. A change on
the Plugins page holds from the next file opened.

## Test and build

```bash
cargo test -p rc-plugin-json-viewer
cargo test -p rc-plugin-json-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh json-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
