# Config viewer

rCommander's bundled viewer plugin for configuration files (RC-100-22). It offers two views:
**highlighted**, the text coloured like an editor, and **tree**, the sections, tables and keys
unfolded.

## Formats

The format is chosen by the file's name (`src/types.rs`, `Format::from_name`):

| Name | Format |
| --- | --- |
| `*.ini` (`desktop.ini`), `*.inf`, `*.cfg`, `*.conf`, `.editorconfig`, `.gitconfig` | INI |
| `*.reg` | Windows registry export |
| `.env`, `*.env` (`.env.local` and the like when the host offers them) | `.env` |
| `*.properties` | Java properties |
| `*.toml`, `Cargo.lock` | TOML |
| `.gitignore`, `.gitattributes` | a pattern a line |

`.lock` is claimed for `Cargo.lock`; `probe` declines every other `.lock` file, so it stays with
the built-in viewer.

## Colours

- **keyword** - a section `[x]`, a TOML table `[a.b]` or array of tables `[[a]]`, a registry key
  `[HKEY_...]`, the registry's version line, `export` in `.env`
- **key** - keys; in a registry export `"Name"` and `@`; in `.gitignore` the pattern
- **string**, **number**, **literal** - values: `true false on off yes no` and TOML dates and
  times are literals; TOML strings of all four kinds, also over lines
- **type-name** - the registry's `dword:`, `hex:`, `hex(2):`
- **comment** - `;` and `#` (`!` in properties)
- **punctuation** - `=`, `:`, brackets, commas, a continuing `\`; **operator** - a registry `-`
  that deletes a value
- **invalid** - what the format has no place for: an unclosed quote or header, `dword:` with other
  than eight hex digits, a TOML value that is no number, boolean or date

What goes on past a line's end is known from line to line: a properties value or registry hex
list continued with `\`, a quoted `.env` value, a TOML multi-line string, and TOML arrays and
inline tables over several lines.

## Tree

Sections and tables are nodes, keys leaves labelled with their key and valued with their value cut
short (`...` when it goes on in the next line). Keys before the first section come first. INI and
registry sections are one level, in the order of the file; TOML tables nest by their dots
(`[a.b]` under `a`), an array of tables shows its elements `[0]`, `[1]`, ..., and a table named
after one (`[a.b]` after `[[a]]`) is under its last element. `.env`, properties and the git files
are flat. More than 1000 children are shown in ranges; at most 16384 sections or tables are kept,
the rest are named in one last node.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
takes the encoding of a byte-order mark (UTF-8, UTF-16 LE or BE - regedit writes UTF-16 LE with
one), else UTF-8 when the first 64 KiB are valid UTF-8, else Windows-1252, as Notepad writes an
ANSI `.ini` on Western European Windows. `utf-8`, `utf-16le` and `windows-1252` read the file in
that encoding whatever it holds. The text the viewer shows is always UTF-8, decoded by
`rc-plugin-text` - search, copy and the token colours work on it. A change on the Plugins page
holds from the next file opened. `.env` values are shown as they are, not masked: the Lister shows
the same file as text anyway.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state and the number of keys
before it, and the sections and tables. Colouring 50 lines in the middle of a 100 MiB file lexes
from the nearest checkpoint, and opening a section reads its keys from the checkpoint before them;
neither reads the file again. A line is coloured up to 64 KiB and 4096 tokens; the rest stays
plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`, `children`) and `probe`, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one, the setting and `Format`
- `src/lexer.rs`, `src/lexer/{ini,reg,env,toml,toml_values}.rs` - the lexer a line at a time and
  the state a line starts in
- `src/entry.rs` - what a line is to the tree: a header, an entry, or neither
- `src/outline.rs` - the sections and tables the open pass finds; `src/tree.rs` - the tree output
- `src/tokens.rs` - the tokens of the lines asked for; `src/text.rs` - the text, decoded
- `src/lines.rs` - the line index; `src/reader.rs` - the windows and lines read, in the file's
  encoding
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the `encoding` setting;
  the fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name, the setting's label and the failure code
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`, importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-config-viewer
cargo test -p rc-plugin-config-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh config-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
