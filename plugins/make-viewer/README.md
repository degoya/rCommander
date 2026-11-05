# Makefile viewer

rCommander's bundled viewer plugin for Makefiles: `Makefile`, `makefile`, `GNUmakefile` and
`*.mk` (RC-100-81). It offers one view, **highlighted**: the file coloured like an editor.

- **function** - the targets of a rule (`all`, `%.o`, `clean`; a target written as `$(OBJ)` is a
  reference), and make's functions in a call: `$(patsubst ...)`, `$(wildcard ...)`,
  `$(shell ...)`, ...
- **keyword** - the directives where a line starts (`include`, `-include`, `sinclude`, `ifeq`,
  `ifneq`, `ifdef`, `ifndef`, `else`, `endif`, `define`, `endef`, `export`, `unexport`,
  `override`, `private`, `undefine`, `vpath`, `load`), the special targets (`.PHONY`,
  `.SUFFIXES`, `.DELETE_ON_ERROR`, ...), and in a recipe the shell's keywords where a command
  starts (`if then else fi for do done case esac while`, ...)
- **key** - the variable an assignment sets, the names after `export` and `ifdef`, and every
  reference: `$(CC)`, `${CFLAGS}`, `$(SRC:.c=.o)`, the automatic variables `$@ $< $^ $? $* $+
  $|`, and `$$name`, a dollar and a variable for the shell
- **operator** - `=`, `:=`, `::=`, `?=`, `+=`, `!=`, a rule's `:` and `::`, the `|` before
  order-only prerequisites, a recipe's prefixes `@`, `-` and `+`, and in a recipe the shell's
  `| || && ; & > >> <`
- **punctuation** - the brackets and commas of a function call and of `ifeq (a,b)`, a rule's `;`
  before a recipe, a recipe's brackets
- **string** - `"..."` and `'...'` in a condition and a recipe, the references in it in their own
  colour (make expands them first)
- **function** in a recipe also - the shell's builtins where a command starts (`echo`, `cd`,
  `test`, `exit`, ...); **comment** - `# ...`, and in a recipe a `#` where a word starts

A recipe line is a line starting with a tab. A line ending in a backslash goes on as what it
was - a recipe, a value, the prerequisites or a comment. The body of a `define` is text up to
its `endef` (nested ones counted); only the references in it are coloured. The recipes get the
shell viewer's colours as far as they need no grammar of their own: no heredocs, no quotes over
lines. The plugin colours, it does not parse; a Makefile that is not well formed is coloured as
far as it reads, never refused.

## Which files it is offered

A Makefile has no extension, so the plugin claims files by their name (`[match] names`,
RC-100-80), in any case, the name alone and never the folder. `Makefile`, `makefile` and
`GNUmakefile` are exact names, asked before every other claim; `*.mk` is a pattern, asked after
the extensions - the plugin also claims the extension `mk`, so only another plugin claiming `mk`
comes before it. Names need rCommander 0.19.0.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8, else `windows-1252`. `utf-8`, `utf-16le` and `windows-1252`
read the file in that encoding whatever it holds; a byte-order mark is skipped only when it is that
encoding's own. The decoding is `rc-plugin-text`'s, shared with the other text viewers. The text
the viewer shows is always UTF-8. A change on the Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - a `define` body
and a line continued. Colouring 50 lines in the middle of a large file lexes from the nearest
checkpoint; it does not read the file again. A line is coloured up to 1 MiB and 4096 tokens; the
rest stays plain. Calls nested more than 32 deep are coloured as one reference, so no line takes
longer than its length.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, statements, assignments, rules and
  values; `src/lexer/directives.rs` conditions and `define`; `src/lexer/refs.rs` references and
  function calls; `src/lexer/recipe.rs` recipe lines; `src/lexer/words.rs` the word tables
- `src/tokens.rs` - the tokens of the lines asked for, a line's text decoded
- `src/lines.rs` - the line index; `src/reader.rs` - the windows read and the lines of each
  encoding
- `manifest.toml` - identity, `[match]` (names and the extension), `[viewer] outputs`, limits and
  the `encoding` setting; the fields every bundled plugin shares are in `../metadata.toml`
- `locales/` - the plugin's name, description and the setting's label in English, German, French
  and Spanish
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-make-viewer
cargo test -p rc-plugin-make-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh make-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it and runs the conformance check; the Windows package carries the
result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
