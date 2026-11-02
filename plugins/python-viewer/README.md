# Python viewer

rCommander's bundled viewer plugin for Python scripts and type stubs, `.py`, `.pyw` and `.pyi`,
and for a script without an extension whose first line runs Python (`#!/usr/bin/env python3`)
(RC-100-74). It offers one view, **highlighted**: the text coloured like an editor.

- **keyword** - `and as assert async await break class continue def del elif else except
  finally for from global if import in is lambda nonlocal not or pass raise return try while
  with yield`, and the soft keywords where they head a statement: `match` and `case` on a line
  ending in `:`, `type` naming an alias (`type Pair[T] = tuple[T, T]`)
- **function** - decorators with their dotted name (`@app.route`), the name after `def`, and
  the built-in functions (`print`, `len`, `isinstance`, `enumerate`, `super`, ...)
- **type-name** - the name after `class` or `type`, the built-in types (`int`, `str`, `list`,
  `dict`, ...) and exceptions and warnings (`ValueError`, `DeprecationWarning`, ...)
- **literal** - `True`, `False`, `None`, `Ellipsis`, `NotImplemented`, `__debug__`
- **string** - `'...'`, `"..."`, `'''...'''` and `"""..."""` over lines, with every prefix in
  any case: `r`, `u`, `b`, `f`, `t`, `rb`, `br`, `fr`, `rf`, `tr`, `rt`; the text of f-strings
  and t-strings with `{{ }}` and their format specs (`:>10`)
- **number** - `42`, `1_000`, `0xFF`, `0o17`, `0b1010`, `1.5`, `.5`, `6.02e23`, and the
  imaginary `2j`
- **comment** - `# ...`; **operator** - `+ - * / // % ** @ := -> == != <= >=` and the others,
  the assignments `= += //= @=`, and a field's conversion `!r`; **punctuation** - brackets,
  braces, `.`, `,`, `:`, `;`, `...`, and the braces of an f-string's fields
- **invalid** - the opening of a string its line ends unclosed, or of a triple-quoted string the
  file leaves open

A name after `.` is an attribute: `os.path.print` colours no built-in. The expressions in an
f-string's fields are coloured as code, read as Python 3.12 reads them: a field may hold strings
in the same quote (`f"{d["key"]}"`), other f-strings, and go on over lines. What reaches from one
line into the next: triple-quoted strings, a string continued by a backslash, f-string fields,
open brackets - a statement goes on inside them - and a backslash at the end of a line.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8 - Python 3's own default - else `windows-1252`, for an old
script saved in the ANSI code page. `utf-8`, `utf-16le` and `windows-1252` read the file in that
encoding whatever it holds; a byte-order mark is skipped only when it is that encoding's own. A
`# -*- coding: ... -*-` line is not read. The decoding is `rc-plugin-text`'s, shared with the
other text viewers. The text the viewer shows is always UTF-8 - search, copy and the token
colours work on it. A change on the Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - the open strings,
fields and brackets and a backslash continuation - and the strings left open. Colouring 50 lines
in the middle of a 100 MiB file lexes from the nearest checkpoint; it does not read the file
again. A line is coloured up to 1 MiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, the strings `open` sees left open;
  `src/lexer/code.rs` statements and expressions, `src/lexer/strings.rs` strings, fields and
  format specs, `src/lexer/words.rs` names, numbers, operators, keywords and built-ins,
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
cargo test -p rc-plugin-python-viewer
cargo test -p rc-plugin-python-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh python-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
