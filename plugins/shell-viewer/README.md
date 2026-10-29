# Shell script viewer

rCommander's bundled viewer plugin for shell scripts - sh, bash, zsh and ksh (RC-100-18). It
offers one view, **highlighted**: the script coloured like an editor.

- **keyword** - `if then elif else fi case esac for select while until do done in function time`,
  `[[ ]]`, and a heredoc's marker where it opens and where it closes
- **function** - builtins (`echo printf read cd export local readonly declare typeset set unset
  shift return exit source . eval exec trap test true false` and the rest of bash's) and the name
  of a function definition (`name() {`, `function name {`)
- **key** - variables (`$x`, `${x...}`, `$1`, `$@`, `$?`, `$$`), the name left of `=` in an
  assignment (`x=1`, `x+=1`, `arr[i]=1`, `local x=1`), a `for` loop's variable, names in
  arithmetic
- **string** - `'...'`, `"..."` with the variables and `$(...)` inside it in their own colours,
  `$'...'`, and a heredoc's text (its variables as `key` unless the marker is quoted)
- **number**, **comment** (the shebang too), **operator** (`| || && ; ;; & > >> < 2>&1 <(` and
  the rest), **punctuation** (brackets, `$(`, `$((`, backticks)
- **invalid** - a quote still open at the end of the file
- a word of no recognisable kind - an external command, an argument - stays uncoloured

A keyword is one only where a command starts, a builtin likewise: in `echo done` neither word is
coloured as a command. The plugin colours, it does not parse; a script that is not well formed
is coloured as far as it reads, never refused.

The text is the file's own, so the plugin has no settings. Bytes that are not UTF-8 show as
U+FFFD. It needs no capability: it reads the file it is handed and nothing else.

## Which files it is offered

rCommander decides from the manifest's `[match]` without running the plugin, then asks `probe`:

- **By extension**: `.sh`, `.bash`, `.zsh`, `.ksh`.
- **Dot files**: rCommander takes what follows a name's last dot as its extension, so
  `.bashrc`, `.bash_profile`, `.profile` and `.zshrc` match by extension - and so does
  `/etc/bash.bashrc`. A name without any dot has no extension: `/etc/profile` is not offered
  unless it starts with a shebang below.
- **By shebang**, for a script without an extension: a file starting with one of `#!/bin/sh`,
  `#!/bin/bash`, `#!/bin/zsh`, `#!/bin/ksh`, `#!/bin/dash`, `#!/usr/bin/bash`, `#!/usr/bin/zsh`,
  `#!/usr/local/bin/bash`, `#!/usr/local/bin/zsh`, `#!/usr/bin/env sh`, `#!/usr/bin/env bash`,
  `#!/usr/bin/env zsh`, `#!/usr/bin/env ksh`, `#!/usr/bin/env -S bash`, `#! /bin/sh` or
  `#! /bin/bash` - the host's limit is 16 such magics. Other spellings (`#! /usr/bin/env bash`,
  `#!/opt/bin/bash`) need the file renamed or assigned in Settings, Plugins
  (`plugins.associations`).
- **`probe`** then takes any text (no NUL in the first 4 KiB) whose first line, if it is a
  shebang, names a shell - `sh`, `bash`, `zsh`, `ksh`, `ksh93`, `mksh`, `pdksh`, `dash` or
  `ash`, directly or through `env` - and declines the rest, which the built-in viewer then shows:
  a binary file, `#!/usr/bin/env python3` in a `.sh` file, `#!/bin/shx`.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
8 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - the open quotes,
`$(`, `${`, `$((`, backticks and `case` frames, a heredoc and its marker (by length and hash), and
where the next word stands when a `\` continued the line. Colouring 50 lines in the middle of a
100 MiB script lexes from the nearest checkpoint and never reads the file again. A line is
coloured up to 64 KiB and 4096 tokens, a token is at most 4 KiB; the rest stays plain.

What a line can start inside of is bounded too: 16 nested frames are kept by kind (deeper ones
are taken for `$(`), one heredoc per line (a second `<<` on the same line is coloured, its text is
not), a heredoc marker up to 1 KiB long.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`) and `probe`, with the host's promises at
  the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs`, `src/lexer/` - the lexer: `command.rs` (words, keywords, operators,
  redirections), `quoted.rs` (quotes, expansions, arithmetic), `heredoc.rs`, `words.rs` (the
  word lists and the shapes looked ahead for), `state.rs` (the state a line starts in)
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
cargo test -p rc-plugin-shell-viewer
cargo test -p rc-plugin-shell-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh shell-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
