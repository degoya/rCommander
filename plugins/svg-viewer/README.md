# SVG viewer

rCommander's bundled viewer plugin for SVG pictures (RC-100-28): `.svg`, and `.svgz` (gzip),
whose `probe` takes a file with `<svg` in its first 64 KiB or a gzip stream, so XML named `.svg`
goes to the next viewer. It offers two views of one document, switched like the viewer's own
modes with **7** or the list in the title row:

- **image** - the picture, drawn with [resvg](https://github.com/linebender/resvg): paths,
  shapes, gradients, patterns, transforms, clipping, masks, filters, text and embedded raster
  images. The longest side is the setting `size` (`512`, `1024` - the default - or `2048`
  pixels), the aspect ratio stays; an SVG without `width` and `height` takes its `viewBox`. The
  setting `background` fills what the picture leaves empty: `transparent` (the default),
  `white` or `black`. The picture leaves the plugin as a raw RGBA raster within the viewer's
  picture limits.
- **highlighted** - the source coloured as the XML viewer colours it (`../xml-viewer/`, whose
  lexer and line index this plugin uses as a library): element and attribute names, values,
  comments, `<?...?>` and `<!DOCTYPE ...>`, entities and punctuation. An `.svgz` shows its
  unpacked XML. Text that is not UTF-8 shows a replacement character, the colours stay on their
  characters.

## What it does not do

- **Nothing outside the file.** An `<image>` is drawn only from a `data:` URL (PNG, JPEG, GIF,
  WebP, or SVG without images of its own); an `href` to a file, a relative path or `http(s)` is
  not followed, and a gzip stream inside a `data:` URL is not unpacked. It needs no capability,
  and the sandbox has neither file system nor network.
- **One font.** The sandbox has no system fonts, so every `font-family` is set in Lato Regular,
  which the plugin brings (`fonts/Lato-Regular.ttf`, SIL Open Font License 1.1, `fonts/OFL.txt`;
  the font carries its copyright and licence in its own `name` table too). Bold and italic are
  drawn in the regular face.
- **No animation, no scripts, no CSS `@import`**: the picture is drawn as it stands.

## Limits

A file of at most 64 MiB, and an `.svgz` unpacked to at most 64 MiB: reading stops one byte past
it, so a gzip bomb costs no more. A document nested deeper than 256 elements
(`depth::MAX_DEPTH`) is refused before anything parses it, since the parser and the renderer
recurse once per level; an SVG inside a `data:` URL too. A document whose `<use>` references would expand to more than
1,048,576 elements (`budget::MAX_EXPANDED_ELEMENTS`) is refused before it is built, counted in one
pass without expanding anything. What is left ends at the host's limits (`manifest.toml`: 256
MiB of memory, 30 s, ten times the default fuel) as a notice, never a hang.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `image`, `text`, `tokens`) and `probe`, with the host's
  promises at the top; the viewer world's types are the XML viewer's
- `src/load.rs` - the file read in windows, `.svgz` inflated under the limit, the bytes as a
  source for the XML viewer
- `src/depth.rs`, `src/budget.rs` - the nesting budget and the `<use>` budget
- `src/render.rs` - usvg's options (the font, `data:` only), the canvas, straight RGBA out
- `src/settings.rs` - `size` and `background`
- `fonts/` - Lato Regular and its licence
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the settings; the fields
  every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and its failure codes
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (contract 0.2.0, which hands the plugin its settings), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-svg-viewer
scripts/build-plugins.sh svg-viewer        # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT; the bundled font OFL-1.1.
