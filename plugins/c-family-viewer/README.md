# C-family viewer

rCommander's bundled viewer plugin for source code of the C family - C, C++, C#, Java, Kotlin,
Go and Rust - in one plugin with one lexer (RC-100-82). It offers one view, **highlighted**: the
text coloured like an editor.

## Languages

The language is the file's extension, in any case; the content is never guessed from.

| Language | Extensions | What it reads beyond the shared core |
| --- | --- | --- |
| C | `.c` | the preprocessor: `#include <path>`, `#define NAME(...)`, directives a backslash carries on, `#error`/`#warning` text; `L"..."`, `u8'x'`; `1'000` |
| C++ | `.cpp` `.cc` `.cxx` `.hpp` `.hh` `.hxx` `.h` `.ino` | C's, and raw strings `R"delim(...)delim"` (also `u8R`, `LR`, ...); a `.h` is read as C++, whose words hold C's |
| C# | `.cs` `.csx` | `#region`, `#if`; verbatim `@"..."` (`""` a quote); interpolated `$"...{x,10:N2}..."` with `{{ }}`; raw `"""..."""` and `$$"""...{{x}}..."""`; `@class` |
| Java | `.java` | annotations `@Override`, `@interface`; text blocks `"""..."""` |
| Kotlin | `.kt` `.kts` | annotations; templates `$name` and `${...}` in strings and raw strings `"""..."""`; nested block comments; a script's `#!` line; `` `quoted names` `` |
| Go | `.go` | raw strings `` `...` `` over lines; runes `'x'` |
| Rust | `.rs` | lifetimes and labels `'a`, `'static` (never a character); raw strings `r#"..."#`, byte and C strings `b"..."`, `c"..."`, `b'x'`; attributes `#[...]`, `#![...]`; macros `name!`; raw names `r#type`; nested block comments; strings over lines |

The shared core: `//` and `/* */` comments (also `///` and `/** */`), strings `"..."` with
backslash escapes, characters `'x'`, numbers with every prefix, separator and suffix (`0xFF`,
`0b1010`, `1_000`, `1.5f`, `6.02e+23`, `10UL`, `255u8`), operators, and brackets.

## Classes

- **keyword** - each language's keywords and the contextual ones a reader expects coloured
  (`var`, `record`, `async`, `override`, ...), `this`/`self`, and directives (`#include`,
  `#region`)
- **type-name** - built-in types (`int`, `size_t`, `string`, `usize`, `Int`, ...), the name
  declared after `class`, `struct`, `enum`, `interface`, `trait`, `record`, `object`, `type`,
  and Rust's lifetimes
- **function** - a name followed by `(`, the name after `fn`/`func`/`fun`/`#define`,
  annotations, a Rust attribute's name and macros with their `!`
- **literal** - `true`, `false`, `null`, `nullptr`, `NULL`, `nil`, `iota`
- **string** - strings, characters, an include's `<path>`, the text of interpolated strings
  and C#'s format specs; **number**, **comment**, **operator**, **punctuation**
- **invalid** - the opening of a one-line string its line ends unclosed, or of a string the
  file leaves open; a quote without its closing quote in reach

What reaches from one line into the next: block comments, the strings that go on over lines,
the fields of interpolated strings, a declaration's name, and - in C and C++ - what a backslash
at the end of a line carries on: a directive, a `//` comment, a string.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8, else `windows-1252`, for an old file saved in the ANSI code
page. `utf-8`, `utf-16le` and `windows-1252` read the file in that encoding whatever it holds; a
byte-order mark is skipped only when it is that encoding's own. The decoding is
`rc-plugin-text`'s, shared with the other text viewers. The text the viewer shows is always
UTF-8 - search, copy and the token colours work on it. A change on the Plugins page holds from
the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there, and the strings
left open. Colouring 50 lines in the middle of a 100 MiB file lexes from the nearest checkpoint;
it does not read the file again. The lexer never goes back and looks ahead a bounded distance
only, so its time is linear in the file. A line is coloured up to 1 MiB and 4096 tokens; the
rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/lang.rs` - `Lang`: the extensions, the rules that differ; `src/lang/tables.rs` the
  keywords, types and constants of each language, sorted
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, the strings `open` sees left open;
  `src/lexer/code.rs` code, `src/lexer/marks.rs` `#`, `@` and `'`, `src/lexer/strings.rs`
  strings, fields and format specs, `src/lexer/words.rs` names, numbers and operators,
  `src/lexer/frames.rs` the open frames
- `src/tokens.rs` - the tokens of the lines asked for, a line's text decoded
- `src/lines.rs` - the line index; `src/reader.rs` - the windows read and the lines of each
  encoding
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the `encoding` setting;
  the fields every bundled plugin shares are in `../metadata.toml`
- `locales/` - the plugin's name, description and the setting's label in English, German, French
  and Spanish
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-c-family-viewer
cargo test -p rc-plugin-c-family-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh c-family-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain: a sample per language, and a 5 MiB file
within the manifest's time limit. `scripts/build-plugins.sh` builds the component for
`wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
