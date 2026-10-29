# YAML viewer

rCommander's bundled viewer plugin for `.yaml` and `.yml` files (RC-100-16). It offers two views
of one document, switched like the viewer's own modes:

- **highlighted** - the text coloured like an editor: keys, strings (quoted and block scalars
  `|`/`>` with their lines), numbers, `true`/`false`/`null`/`~` and the like, comments, document
  markers `---`/`...` and directives, anchors `&a`, aliases `*a` and tags `!!str`, the
  indicators `-` `:` `[` `]` `{` `}` `,` and anything invalid each in its theme colour. A plain
  scalar of no recognisable kind stays uncoloured.
- **tree** - the structure to unfold by indentation: a key or `[index]` per node, a scalar,
  alias, flow collection (`[...]`, `{...}`) or block scalar as its shortened value, `{n}` or
  `[n]` for a mapping or sequence of `n` children. Several documents show as `document 1`,
  `document 2`, ... A collection of more than 1000 children unfolds in ranges (`[0..999]`, ...).
  Anchors and aliases are shown as written, never resolved, so no cycle or expansion bomb can
  reach the tree. A file that is not well formed - bad indentation, a tab in the indentation, an
  unterminated quote - shows a first node naming the line and column of the first error, and the
  tree lists what came before it.

YAML is not reformatted, so the plugin has no settings. It needs no capability: it reads the file
it is handed and nothing else.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - inside a block
scalar and its indentation, inside a quoted scalar, the depth of open flow collections - and, for
the large collections and values, their end, their child count and the offset of every 1000th
child. Colouring 50 lines in the middle of a 100 MiB file lexes from the nearest checkpoint, a
page of the tree reads from the nearest mark; neither reads the file again. A line is coloured up
to 64 KiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`, `children`) and `probe`, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs`, `src/lexer/scalars.rs` - the lexer and the state a line starts in
- `src/tokens.rs` - the tokens of the lines asked for
- `src/lines.rs` - the line index
- `src/events.rs`, `src/structure.rs`, `src/walk.rs`, `src/tree.rs` - keys, items and values,
  the structure pass and its index, the walks and the tree
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits; the fields every bundled
  plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and its failure codes
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`; the `settings` import is never called, so the component does not
  import it), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-yaml-viewer
cargo test -p rc-plugin-yaml-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh yaml-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
