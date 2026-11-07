# Script viewer

rCommander's bundled viewer plugin for scripts in Ruby, Perl, Lua and VBScript, in one plugin
with a lexer per language (RC-100-83). It offers one view, **highlighted**: the text coloured
like an editor.

## Languages

The language is the file's name, in any case; the content is never guessed from. `Gemfile` and
`Rakefile` have no extension and are claimed by their name (`[match] names`, rCommander 0.19.0
or later).

| Language | Names | What it reads |
| --- | --- | --- |
| Ruby | `.rb`, `Gemfile`, `Rakefile` | `#` comments and `=begin` ... `=end`; strings in `"`, `'` and `` ` ``, and `%w[...]`, `%q(...)`, `%Q{...}`, `%i[...]`, `%r{...}`, `%(...)` with nested brackets; interpolation `#{...}` lexed as code and `#@name`, `#$name`; symbols `:name`, `:"..."`; hash keys and keyword arguments `name:`; `@name`, `@@name`, `$name`, `$0`; regular expressions with their flags where no value stands before the `/`; heredocs `<<~TAG`, `<<-TAG`, `<<TAG`, `<<'TAG'`; the name after `def` (also `def self.name`), `class` and `module`; constants; methods ending in `?` and `!`; `__END__` |
| Perl | `.pl`, `.pm` | `#` comments and POD from `=head1`, `=pod`, ... to `=cut`; strings and `q`, `qq`, `qw`, `qx`, `qr`, `m`, `s`, `tr`, `y` with any delimiter, brackets nested, two parts for `s` and `tr`, flags; `$scalar`, `@array`, `%hash`, `$#last`, `$_`, `$1`, `@_`, interpolated in strings and patterns; patterns where no value stands before the `/`, `//` after one; heredocs `<<"TAG"`, `<<'TAG'`, `<<TAG`, `<<~TAG`; `sub name`, `package` and `use` with the module, `Foo->new`, `&name`; barewords before `=>` and in `{key}`; `<STDIN>`; `__END__`/`__DATA__` with POD after them |
| Lua | `.lua` | `--` comments and long comments `--[[ ... ]]`, `--[==[ ... ]==]`; long strings `[[ ... ]]` at their level; short strings with escapes, carried on by a `\` or `\z` at the line's end; hexadecimal numbers with fractions and `p` exponents; `#`, `..`, `...`, `~=`; labels `::name::`; the name after `function` in all its parts (`function M.a:b()`); a script's `#!` line |
| VBScript | `.vbs` | keywords and constants in any case; `'` and `Rem` comments; strings with `""` for a quote; dates `#1/31/2026#`; `&H1F`, `&O17`; the name after `Sub`, `Function`, `Property Get\|Let\|Set` and `Class` - not after `End` or `Exit`; built-in functions (`MsgBox`, `CreateObject`, ...); ` _` continuing a line |

## Classes

- **keyword** - each language's keywords (`def`, `my`, `local`, `Dim`, ...), `__END__`
- **key** - variables (`@items`, `$total`, `%prices`), hash keys and barewords before `=>`,
  Lua's labels, Perl's handles `<STDIN>`
- **literal** - `nil`, `true`, `false`, `undef`, `Nothing`, `Null`, `Empty`, Ruby's symbols
- **type-name** - constants, classes and modules, the name after `class`, `module`, `package`,
  `use`, VBScript's `Class`
- **function** - a name followed by `(`, the name `def`, `sub`, `function`, `Sub` and `Function`
  declare, methods after `->`, and the functions every script of the language knows (`puts`,
  `print`, `ipairs`, `MsgBox`, ...)
- **string** - strings, quote-like operators, patterns, heredocs with their tag lines;
  **number**, **comment**, **operator**, **punctuation**
- **invalid** - the opening of a string its line ends unclosed (Lua, VBScript), or of a string
  or heredoc the file leaves open

What reaches from one line into the next: a string over lines (Ruby and Perl strings may hold
line breaks), a heredoc's body, `=begin` and POD, a Lua long bracket, the data after `__END__`.
An interpolation is lexed up to its brace on the same line, at most four deep; a second heredoc
opened on the same line has no body of its own.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8, else `windows-1252`, for an old script saved in the ANSI code
page. `utf-8`, `utf-16le` and `windows-1252` read the file in that encoding whatever it holds; a
byte-order mark is skipped only when it is that encoding's own. The decoding is
`rc-plugin-text`'s, shared with the other text viewers. The text the viewer shows is always
UTF-8 - search, copy and the token colours work on it. A change on the Plugins page holds from
the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there, and the strings
left open. Colouring 50 lines in the middle of a 100 MiB file lexes from the nearest checkpoint;
it does not read the file again. The lexers never go back and look ahead a bounded distance only
(a heredoc's tag of at most 32 bytes, an interpolation's brace on its line), so their time is
linear in the file. A line is coloured up to 1 MiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/lang.rs` - `Lang`: the names and extensions; `src/lang/tables.rs` the keywords, constants
  and well-known functions of each language, sorted
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the state a line starts in, the strings `open` sees left open, the dispatch;
  `src/lexer/ruby.rs`, `perl.rs`, `lua.rs`, `vbs.rs` a language each, `src/lexer/quoted.rs`
  strings, interpolation and heredocs of Ruby and Perl, `src/lexer/scan.rs` the cursor, names,
  numbers and operators
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
cargo test -p rc-plugin-script-viewer
cargo test -p rc-plugin-script-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh script-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain: a sample per language, and a 5 MiB file
within the manifest's time limit. `scripts/build-plugins.sh` builds the component for
`wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
