# Web code viewer

rCommander's bundled viewer plugin for the web languages - HTML, CSS, JavaScript, TypeScript and
PHP, and their relatives (RC-100-26). It offers one view, **highlighted**: the code coloured like
an editor, the languages inside each other too.

## Languages

| Extension | Read as |
| --- | --- |
| `.html`, `.htm`, `.xhtml`, `.vue` | HTML: `<script>` as JavaScript, `<style>` as CSS; `lang="ts"` and `lang="scss"` (Vue) or `type="text/typescript"` choose the dialect, a `type` that is no script (`text/x-template`) stays plain |
| `.css`, `.scss`, `.less` | CSS; SCSS and LESS with nesting, `$x` and `@x` variables, `//` comments, `#{...}` |
| `.js`, `.mjs`, `.cjs` | JavaScript |
| `.jsx` | JavaScript with JSX elements |
| `.ts`, `.mts`, `.cts` | TypeScript: type annotations and type arguments |
| `.tsx` | TypeScript with JSX elements |
| `.php`, `.phtml` | HTML with `<?php ... ?>` and `<?= ... ?>` anywhere: in text, attributes, scripts and styles |

Not `.json` (the JSON viewer) and not `.svg` or `.xml` (the XML viewer). A file assigned to the
plugin under another name (Settings, Plugins, `plugins.associations`) is read as PHP when an
opening tag is in its first 4 KiB, as HTML when it starts with `<`, else as JavaScript.

## Classes

The same class means the same in every language:

- **keyword** - keywords (PHP's in any case), at-rules (`@media`), a PHP heredoc's markers,
  `<!DOCTYPE>`
- **key** - attribute names, CSS properties and media features, object keys and labels, PHP
  `$variables` (in double-quoted strings and heredocs too), SCSS `$x` and LESS `@x` variables
- **string** - strings, template strings, attribute values, regular expressions, `url(...)`
- **number** - numbers, CSS measures (`2em`, `50%`) and colours (`#fff`)
- **literal** - `true false null undefined NaN`, `!important` and CSS keyword values (`auto`,
  `none`, `inherit`, ...), HTML entities (`&amp;`)
- **comment** - `//`, `/* */`, `<!-- -->`, PHP's `#`, a `#!` line
- **type-name** - element names, CSS selectors (`.class`, `#id`, `:hover`, `&`), classes and
  interfaces, TypeScript annotations and type arguments, PHP's built-in types and namespaces
- **function** - functions where called or defined, CSS functions (`calc(`), mixins, decorators
- **operator**, **punctuation** - brackets, `;`, `,`, `<` `>` of tags and type arguments,
  `${`, `<?php`, `?>`
- **invalid** - what the lexer cannot read, and the rest of a line nested deeper than 64
- text, plain names and properties after a `.` stay uncoloured

**A regular expression or a division**: a `/` starts a regular expression where no value comes
before it (at the start, after an operator, `(`, `,`, `=` or a keyword like `return`), a division
after a name, a number, a string, `)` or `]` - the usual heuristic, which holds across lines.

**Where an embedded language ends** is where the browser ends it: `</script` ends a script even
in a string or a comment, an attribute's quote ends the CSS or JavaScript in it. PHP ends at `?>`
in code and in a `//` or `#` comment, not in a string or a `/* */` comment.

The plugin colours, it does not parse: a file that is not well formed is coloured as far as it
reads, never refused. The text is the file's own, so the plugin has no settings. Bytes that are
not UTF-8 show as U+FFFD. It needs no capability: it reads the file it is handed and nothing
else.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
8 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - the stack of
languages the line starts inside of and where each goes on (a comment, a string, a tag's
attributes, a declaration's value, a heredoc). Colouring 50 lines in the middle of a 100 MiB
file lexes from the nearest checkpoint and never reads the file again. A line is coloured up to
64 KiB and 4096 tokens - a minified file's one line too -, a token is at most 4 KiB; the rest
stays plain.

What a line can start inside of is bounded too: the languages nest at most 64 deep (a template
string in a template string's `${...}` counts two); a deeper one is not opened and the rest of
its line is `invalid`.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`) and `probe`, with the host's promises at
  the top; `src/language.rs` - the language by extension or first bytes
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs`, `src/lexer/` - the lexer: the frames and the boundaries between them in
  `lexer.rs`, a module per language - `html.rs`, `css.rs`, `js.rs` and `js_words.rs`, `jsx.rs`
  (template strings and JSX), `php.rs` - with `scan.rs` (what they share), `words.rs` (the word
  lists) and `state.rs` (the stack a line starts in)
- `src/tokens.rs` - the tokens of the lines asked for
- `src/lines.rs` - the line index
- `src/reader.rs`, `src/utf8.rs` - the windows read, and the UTF-8 the text is written in
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits; the fields every bundled
  plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and description
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`; the `settings` import is never called, so the component does not
  import it), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-web-viewer
cargo test -p rc-plugin-web-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh web-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
