# Dockerfile viewer

rCommander's bundled viewer plugin for Dockerfiles and Containerfiles (RC-100-80). It offers one
view, **highlighted**: the file coloured like an editor.

- **keyword** - the instructions (`FROM RUN CMD ENTRYPOINT SHELL ENV ARG LABEL COPY ADD WORKDIR
  USER VOLUME EXPOSE STOPSIGNAL MAINTAINER HEALTHCHECK ONBUILD`, in any case), `AS` in `FROM`,
  `CMD` and `NONE` in `HEALTHCHECK`, a heredoc's marker where it opens and where it closes
- **type name** - the image in `FROM`; **function** - the stage's name after `AS`
- **key** - flags (`--platform`, `--from`, `--mount`, `--chown`), the keys of `ENV`, `ARG` and
  `LABEL`, variables (`$VAR`, `${VAR:-default}`), a parser directive's name
- **string** - quoted text, the values of flags and pairs, a parser directive's value, the body
  of a heredoc that is a file's content (`COPY <<EOF /etc/x`)
- **number** - ports (`EXPOSE 8080/tcp`), a numeric user
- **punctuation** - the exec form's `[`, `]` and `,`, the escape character that continues a line
- **comment** - a line starting with `#`
- **invalid** - a word where an instruction belongs that Docker does not know

The shell form of `RUN`, `CMD`, `ENTRYPOINT` and `HEALTHCHECK CMD`, and the body of a
`RUN <<EOF` heredoc, get the shell viewer's colours as far as a line shows them: keywords
(`if then fi for do done`), builtins as functions (`cd export echo set`), operators (`&& || | ;
> 2>&1`), variables, strings, numbers and comments. What only the shell viewer's own lexer keeps
over lines (substitutions, `case` patterns) is not followed - no second grammar. A heredoc that
feeds a command (`RUN python3 <<EOF`) is that command's input and shows as a string.

Parser directives (`# syntax=`, `# escape=`, `# check=`) count only before anything else, as
Docker reads them; `# escape=`` ` makes the backtick the escape character that continues a line
(Windows Dockerfiles). A comment line inside a continued instruction is a comment, and the
instruction goes on after it. The plugin colours, it does not parse; a Dockerfile that is not
well formed is coloured as far as it reads, never refused.

The text is the file's own, so the plugin has no settings. Bytes that are not UTF-8 show as
U+FFFD. It needs no capability: it reads the file it is handed and nothing else.

## Which files it is offered

rCommander decides from the manifest's `[match]` without running the plugin, then asks `probe`:

- **By name** (`[match] names`, rCommander 0.19.0 and later), without regard to case:
  `Dockerfile` and `Containerfile` before every extension; `Dockerfile.*` and
  `Containerfile.*` after the extensions, so `Dockerfile.dev` and `Dockerfile.prod` come here
  while `Dockerfile.toml` stays a TOML file.
- **By extension**: `.dockerfile`, `.containerfile` (`app.dockerfile`).
- **`probe`** then takes any text (no NUL in the first 4 KiB) and declines a binary file, which
  the built-in viewer then shows.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
8 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - the escape
character, whether directives may still come, the instruction a line continues, a quote it
leaves open, and up to two heredocs with their markers (up to 24 bytes each; a longer marker is
not taken for a heredoc). Colouring 50 lines in the middle of a large file lexes from the nearest
checkpoint and never reads the file again. A line is coloured up to 64 KiB and 4096 tokens; the
rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`) and `probe`, with the host's promises at
  the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs`, `src/lexer/` - the lexer, a line at a time: `args.rs` (exec form, `FROM`,
  pairs, the rest), `shell.rs` (the shell form), `cursor.rs` (blanks, variables, strings, flags,
  heredoc markers)
- `src/scan.rs` - the lines read, each with the last byte that says whether it continues
- `src/tokens.rs` - the tokens of the lines asked for
- `src/lines.rs` - the line index
- `src/reader.rs`, `src/utf8.rs` - the windows read, and the UTF-8 the text is written in
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits; the fields every bundled
  plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and description
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.3.0`; the `settings` import is never called, so the component does not
  import it), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-dockerfile-viewer
scripts/build-plugins.sh dockerfile-viewer        # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it and runs the conformance check; the Windows package carries the
result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
