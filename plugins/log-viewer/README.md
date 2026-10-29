# Log viewer

rCommander's bundled viewer plugin for log files, `.log`, `.out` and `.err` (RC-100-25). It
offers one view, **highlighted**: the log coloured so the errors stand out, also in files of
hundreds of MiB.

- **Levels** - `ERROR FATAL CRITICAL SEVERE PANIC` and syslog's `ERR CRIT ALERT EMERG` as
  **invalid** (red), `WARN WARNING` as **function** (yellow in the dark theme, ochre in the light
  one), `INFO NOTICE` as **type-name**, `DEBUG TRACE VERBOSE` as **comment** (muted). A level is a
  whole word in upper, lower or capitalised case (`ERROR`, `error`, `Error`): `ERRORS`,
  `information`, `error.log` or `on-error` are none. Also inside brackets (`[ERROR]`), as a value
  (`level=error`, `"level":"error"`), and as the letter of Android's logcat (`E/Tag:`,
  `10-20 15:20:01.123 1234 1234 E Tag:`) and of glog (`E1020 15:20:01.123456`).
- **number** - timestamps: ISO 8601 (`2026-10-20T15:20:01.123+02:00`, with a space, a `,`
  fraction or `/`), a clock (`15:20:01.123`), syslog and ctime (`Oct 20 15:20:01`), Apache
  (`[20/Oct/2026:15:20:01 +0200]`), Unix seconds at a line's start (`1792502401.5`).
- **literal** - IPv4 (with a port) and IPv6 addresses, URLs, Windows paths (`C:\x`, `\\server\x`)
  and Unix paths (`/var/log`, `./x`, `~/x`).
- **key** - `key=value` and JSON's `"key":`; **string** - text in `"..."` or `'...'` (a quoted
  level or timestamp takes its own class); **punctuation** - `[` and `]`.
- **type-name** - stack-trace lines as a whole: Java and .NET (`at x.Y.m(Y.java:10)`,
  `Caused by:`, `Suppressed:`, `... 3 more`), JavaScript (`at f (/x.js:1:2)`), Python
  (`Traceback (most recent call last):`, `File "x.py", line 3, in m`).

## Settings

- **Level colour** (`level_colour`): `word`, the default, colours the level word alone. `line`
  colours the whole line of an error or a warning in the level's colour, and the lines of its
  stack trace after it: frames, indented lines and the line naming the exception
  (`java.io.IOException: x`, `ValueError: bad`). A Python traceback is an error even without a
  level before it. The next record, an empty line or any other line ends it.
- **Encoding** (`encoding`): `auto`, the default, takes a byte-order mark (UTF-8, UTF-16), else
  UTF-8 when the whole file is valid UTF-8, else Windows-1252. `utf-8`, `utf-16le` and
  `windows-1252` read the file in that encoding whatever it holds. The encodings come from the
  shared crate `rc-plugin-text`. The text the viewer shows is always UTF-8 - search, copy and the
  token colours work on it; a byte that is not valid shows as U+FFFD.

A change on the Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
8 MiB: a checkpoint every 64 lines (or 64 KiB), with `line` the record whose stack trace may go on
there, and checks on the way whether the file is UTF-8. With `word` the pass lexes nothing: a
line's colours then depend on that line alone. Colouring 50 lines in the middle of a 100 MiB log
reads from the nearest checkpoint; it does not read the file again, and a line is lexed once from
its start. A line is coloured up to 64 KiB and 4096 tokens; the rest stays plain.

The plugin is offered logs of up to 1 GiB. The text a viewer plugin writes stops at
`plugins.viewer_output_limit_mib` (256 MiB unless raised on the Plugins page), as for every
viewer: the Lister shows a larger log up to there.

## Which files

rCommander takes what follows the last dot of a name as its extension: `app.log`, `nohup.out`
and `app.err` are offered, a rotated `app.log.1` or `app.log.2026-10-20` is not (its extension is
`1`, `2026-10-20`). `probe` declines a file with a NUL byte in its first 4 KiB (`a.out`, a
program) unless it starts with a UTF-16 byte-order mark or the encoding is set to `utf-16le`.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`) and `probe`, with the host's promises at the
  top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one, and the settings `EncodingSetting` and `LevelColour`
- `src/lexer.rs` - the lexer, levels, keys and strings, and the state a line starts in;
  `src/lexer/stamps.rs` timestamps, `src/lexer/addresses.rs` addresses, URLs and paths,
  `src/lexer/trace.rs` stack traces
- `src/tokens.rs` - the tokens of the lines asked for; `src/decode.rs` - a line as text
- `src/lines.rs` - the line index; `src/reader.rs` - the windows and lines read, by byte or by
  UTF-16 unit
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits and the two settings; the
  fields every bundled plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and the settings' labels
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`, importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-log-viewer
cargo test -p rc-plugin-log-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh log-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
