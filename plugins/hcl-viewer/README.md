# HCL viewer

rCommander's bundled viewer plugin for HCL, the HashiCorp configuration language: Terraform's
`.tf` and `.tfvars`, and every other `.hcl` file - Packer templates, Nomad jobs, Terragrunt,
Vault policies, `.terraform.lock.hcl` (RC-100-83). It offers one view, **highlighted**: the file
coloured like an editor. A `.tfvars.json` is JSON and opens in the JSON viewer.

- **keyword** - a block's type, the first word of a line that labels and a `{` follow
  (`resource "aws_instance" "web" {`, `locals {`, `dynamic "setting" {`); the words of `for`
  expressions and template directives (`for`, `in`, `if`, `else`, `endif`, `endfor`); a
  heredoc's marker (`EOT` of `<<EOT` and its closing line)
- **key** - an attribute's name, a word `=` follows (`ami = ...`, `Name = ...` in an object)
- **string** - quoted templates `"..."` with backslash escapes, a block's labels, and a
  heredoc's lines (`<<EOT` and `<<-EOT` up to the line that holds only the marker); `$${` and
  `%%{` are text
- **punctuation** - the brackets, `,`, `.`, and the openings and closings of template sequences
  `${ ... }` and `%{ ... }` (with their strip markers `${~` and `~}`); the code inside a sequence
  is coloured as code, strings in it included
- **type-name** - the type constraints `string`, `number`, `bool`, `any`, `list`, `map`, `set`,
  `object`, `tuple`
- **literal** - `true`, `false`, `null`
- **function** - a name a bracket follows right away: `merge(`, `lookup(`, `file(`
- **number** - `42`, `1.5`, `6.02e23`; **comment** - `# ...`, `// ...` and `/* ... */` over
  lines; **operator** - `= == != <= >= && || => ? : ! + - * / % ...` and a heredoc's `<<`
- **invalid** - the opening quote of a quoted template its line leaves open, which HCL refuses
- a name of no kind - `var`, `local`, a resource's name, an attribute after `.` - stays
  uncoloured

The plugin colours, it does not parse: a file that is not well formed is coloured as far as it
reads, never refused.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8 - what HCL's specification demands - else `windows-1252`, for a
file saved in the ANSI code page. `utf-8`, `utf-16le` and `windows-1252` read the file in that
encoding whatever it holds; a byte-order mark is skipped only when it is that encoding's own. The
decoding is `rc-plugin-text`'s, shared with the other text viewers. The text the viewer shows is
always UTF-8 - search, copy and the token colours work on it. A change on the Plugins page holds
from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - a block comment, a
heredoc and its marker, a template sequence still open. Colouring 50 lines in the middle of a
100 MiB file lexes from the nearest checkpoint; it does not read the file again. A line is
coloured up to 1 MiB and 4096 tokens; the rest stays plain. Templates nest at most eight frames
deep; a string opened deeper takes the rest of its line.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the frames a line starts inside of, comments, template text and
  heredoc markers; `src/lexer/code.rs` words, blocks, numbers, operators, brackets and what
  opens a comment, a template or a heredoc; `src/lexer/words.rs` the tables of keywords, types
  and literals
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
cargo test -p rc-plugin-hcl-viewer
cargo test -p rc-plugin-hcl-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh hcl-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it and runs the conformance check; the Windows package carries the
result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
