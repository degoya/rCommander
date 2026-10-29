# PowerShell viewer

rCommander's bundled viewer plugin for PowerShell scripts, modules and module manifests, `.ps1`,
`.psm1` and `.psd1` (RC-100-23). It offers one view, **highlighted**: the text coloured like an
editor, case-insensitive as PowerShell reads it.

- **keyword** - where a statement starts: `if elseif else switch foreach for while do until
  function filter param begin process end return break continue try catch finally throw trap
  class enum using exit`
- **function** - cmdlets and advanced functions by their `Verb-Noun` name (`Get-ChildItem`,
  `ConvertTo-Json`) where a command starts, and the name after `function` or `filter`
- **key** - variables: `$x`, `$_`, `$env:PATH`, `$script:x`, `${a b}`, `$$`, `$?`, and
  splatting `@args`; **literal** - `$true`, `$false`, `$null`
- **type-name** - types and attributes: `[string]`, `[System.IO.File]`, `[int[]]`,
  `[CmdletBinding()]`, and the name after `class` or `enum`
- **operator** - parameters `-Name`, the operators `-eq -ne -like -match -and -or -not -f` and
  all others written with a dash, `| ; && || = += .. ::`, arithmetic, and the redirections
  `> >> 2>&1 *>`
- **string** - `'...'` and `"..."`, split around the variables and `$( )` they expand; the
  here-strings `@" ... "@` and `@' ... '@` over lines; **number** - `42`, `1.5e3`, `0x1F`,
  `10kb`, `1.5GB`, `42l`
- **comment** - `# ...` and `<# ... #>` over lines (with `.SYNOPSIS` and the other help
  keywords); **punctuation** - brackets, braces, `$(`, `@(`, `@{`, commas
- **invalid** - the opening of a string or here-string the file leaves open

A command's arguments stay uncoloured but for numbers, variables, strings and parameters: a word
that spells a keyword or a cmdlet after a command's name is an argument. What reaches from one
line into the next: open strings and here-strings, a block comment, `$( )` and `( )` with the
strings inside them, and a backtick at the end of a line, so the next line goes on as the
command it continues. A here-string's closer stands at the start of a line, as PowerShell wants
it.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8, else `windows-1252` - the way Windows PowerShell 5.1 reads and
saves a script without a mark. `utf-8`, `utf-16le` and `windows-1252` read the file in that
encoding whatever it holds; a byte-order mark is skipped only when it is that encoding's own.
A UTF-16 file without a mark needs `utf-16le`. The decoding is `rc-plugin-text`'s, shared with
the other text viewers. The text the viewer shows is always UTF-8 - search, copy and the token
colours work on it. A change on the Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - the open frames
and a backtick continuation - and the strings still open at the end. Colouring 50 lines in the
middle of a 100 MiB file lexes from the nearest checkpoint; it does not read the file again. A
line is coloured up to 1 MiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, the strings `open` sees left open;
  `src/lexer/code.rs` statements, commands and expressions, `src/lexer/strings.rs` strings,
  here-strings and comments, `src/lexer/words.rs` names, variables, numbers, keywords,
  `src/lexer/frames.rs` the open frames
- `src/tokens.rs` - the tokens of the lines asked for, a line's text decoded
- `src/lines.rs` - the line index; `src/reader.rs` - the windows read and the lines of each
  encoding
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the `encoding` setting;
  the fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and the setting's label
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`, importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-powershell-viewer
cargo test -p rc-plugin-powershell-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh powershell-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
