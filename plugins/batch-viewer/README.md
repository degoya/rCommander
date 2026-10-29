# Batch viewer

rCommander's bundled viewer plugin for Windows batch files, `.bat` and `.cmd` (RC-100-19). It
offers one view, **highlighted**: the text coloured like an editor, case-insensitive as cmd.exe
reads it.

- **keyword** - command names where a command starts (`if else for in do goto call set setlocal
  endlocal echo exit shift pushd popd cd chdir start title color cls pause choice`), `if`'s
  `not exist defined errorlevel cmdextversion`, and the switches `/a /p /i /f /d /r /l`
- **operator** - `|` `||` `&` `&&` and the redirections `>` `>>` `<` `2>&1` `2>nul`, and `if`'s
  comparisons `==` `equ` `neq` `lss` `leq` `gtr` `geq`
- **function** - labels: `:label`, and the label after `goto` or `call :label`
- **key** - variables: `%x%`, `%1`, `%*`, `%~dp0`, `%~$PATH:1`, `%%i`, `%%~nxi`, `!x!`, and the
  substitutions `%x:a=b%` and `%x:~0,3%`
- **string** - `"..."`, split around the variables in it; **number** - a whole argument of digits
  or `0x` hexadecimal
- **comment** - `rem ...` (also `rem.`, never `remark`) and `:: ...`
- **punctuation** - `@` where a command starts, the block brackets `(` `)`

`echo`'s text stays uncoloured but for its variables; an operator inside quotes or escaped with
`^` is text. Two things reach from one line into the next: a `^` at the end of a line, so the next
line goes on as the command it continues, and open `( ... )` blocks, inside which a `)` closes the
block even in `echo`'s text. A comment ends with its line; cmd.exe knows no string over several
lines.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
takes UTF-8 when the file is valid UTF-8 or starts with a byte-order mark, else `cp850`, the OEM
code page batch files of German Windows installations are written in. `utf-8`, `cp850`, `cp437`
and `windows-1252` read the file in that encoding whatever it holds. The three code pages come
from `rc-plugin-text`, the encodings the bundled plugins share. The text the viewer shows is always UTF-8 - search, copy and
the token colours work on it. A change on the Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - the depth of open
blocks and a `^` continuation - and checks on the way whether the file is UTF-8. Colouring 50
lines in the middle of a 100 MiB file lexes from the nearest checkpoint; it does not read the file
again. A line is coloured up to 64 KiB (cmd.exe itself reads 8191 characters) and 4096 tokens;
the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`) and `probe`, with the host's promises at the
  top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one, and the setting `Encoding`
- `src/encoding.rs` - the file's bytes as UTF-8 text a line at a time (`rc-plugin-text`'s
  `Decoder`), and the UTF-8 check of `auto`
- `src/lexer.rs`, `src/lexer/parts.rs` - the lexer, the state a line starts in, and the pieces of
  a command
- `src/tokens.rs` - the tokens of the lines asked for
- `src/lines.rs` - the line index; `src/reader.rs` - the windows and lines read
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the `encoding` setting;
  the fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and the setting's label
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`, importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-batch-viewer
cargo test -p rc-plugin-batch-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh batch-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
