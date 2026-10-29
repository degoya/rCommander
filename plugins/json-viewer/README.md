# JSON viewer

rCommander's bundled viewer plugin for `.json` and `.geojson` files (RC-100-11). It offers two
views of one document, switched like the viewer's own modes:

- **highlighted** - the text coloured like an editor: keys, strings, numbers, `true`/`false`/
  `null`, punctuation and anything invalid each in its theme colour. The setting `format` shows
  the text as it is in the file (`as_is`, the default) or indented two spaces a level
  (`pretty`), which turns a minified file into readable lines.
- **tree** - the structure to unfold: a key or `[index]` per node with its value, `{n}` or `[n]`
  for a container of `n` children. A container of more than 1000 children unfolds in ranges
  (`[0..999]`, ...). Invalid JSON shows a first node naming the line and column of the first
  error, and the tree lists what came before it.

It needs no capability: it reads the file it is handed and nothing else.

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
- `src/lexer.rs`, `src/tokens.rs` - the lexer and the tokens of one line
- `src/lines.rs`, `src/pretty.rs`, `src/text.rs` - the line index, the pretty printer, the text
- `src/structure.rs`, `src/walk.rs`, `src/tree.rs` - validation, the index of large values and
  the tree
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
