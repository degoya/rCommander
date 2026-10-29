# XML viewer

rCommander's bundled viewer plugin for XML files (RC-100-15): `.xml`, `.xsd`, `.xsl`, `.xslt`,
`.rss`, `.atom`, the MSBuild and NuGet files (`.csproj`, `.vbproj`, `.vcxproj`, `.props`,
`.targets`, `.nuspec`), `.resx`, `.xaml`, `.config`, `.manifest` and `.plist`; not `.svg` (an
image) or `.html`. It offers two views of one document, switched like the viewer's own modes:

- **highlighted** - the text coloured like an editor: element names, attribute names, attribute
  values and CDATA, comments, `<?...?>` and `<!DOCTYPE ...>`, entities such as `&amp;`, the
  punctuation `<`, `</`, `>`, `/>` and `=`, and anything malformed, each in its theme colour;
  text between tags keeps the text colour. A comment, CDATA section or tag over several lines is
  coloured as one. The setting `format` shows the text as it is in the file (`as_is`, the
  default) or laid out (`pretty`): an element a line, two spaces a level, an element holding only
  text on one line, which turns a one-line generated file into readable lines. Comments, CDATA
  sections and attribute values over several lines keep their own lines.
- **tree** - the elements to unfold: a node per element labelled with its start tag, the first
  attributes included and cut at 80 characters, and for an element without child elements its
  text as the value (whitespace folded, `&lt;` and the other predefined entities decoded).
  Comments and processing instructions are no nodes. An element of more than 1000 children
  unfolds in ranges (`[0..999]`, ...). XML that is not well-formed shows a first node naming the
  line and column of the first error - an element left open, an end tag that does not match - and
  the tree lists what came before it.

"Well-formed" is what one pass can tell without a DTD: tags nested and closed by the same name,
one root element, attributes as `name="value"`, comments, CDATA sections, processing
instructions and the DOCTYPE closed, entity references well-formed. Undeclared entities,
duplicate attributes and namespace prefixes are not checked. The text is shown as UTF-8; a file
in UTF-16 is not taken by the plugin (`probe` looks for a `<` first) and opens in the built-in
viewer.

It needs no capability: it reads the file it is handed and nothing else.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there, one after every
line longer than 64 KiB, and, for the large elements, their end, their child count and the offset
of every 1000th child. Colouring 50 lines in the middle of a 100 MiB file reads from the nearest
checkpoint, a page of the tree from the nearest anchor; neither reads the file again. A line is
coloured up to 64 KiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`, `children`) and `probe`, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs`, `src/lexer/scan.rs`, `src/tokens.rs` - the lexer (a lexeme never spans a line;
  its mode carries a construct to the next) and the tokens of one line
- `src/lines.rs`, `src/pretty.rs`, `src/text.rs` - the line index, the pretty printer, the text
- `src/structure.rs`, `src/walk.rs`, `src/tree.rs` - the well-formedness check, the index of
  large elements and the tree
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the `format` setting;
  the fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and its failure codes
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (contract 0.2.0, which hands the plugin its `format` setting), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-xml-viewer
cargo test -p rc-plugin-xml-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh xml-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
