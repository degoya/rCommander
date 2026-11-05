# Server config viewer

rCommander's bundled viewer plugin for server configuration files (RC-100-81): nginx, Apache
httpd and `.htaccess`, OpenSSH, crontabs and `hosts`. It offers one view, **highlighted**: the
file coloured like an editor, each in its own grammar.

- **nginx** (`nginx.conf`, `*.nginx`, an nginx-like `.conf`) - a statement is a directive and its
  values up to `;`, or up to `{` for a block, and may go on over lines: the directive is **key**,
  a block's (`http`, `server`, `location`, `upstream`, `if` ...) **keyword**; `$host`,
  `${name}` and `$1` **function**; `= ~ ~* ^~` **operator**; `; { } ( )` **punctuation**;
  `# ...` where a word would start **comment**; quoted strings, also over lines
- **Apache** (`httpd.conf`, `apache2.conf`, `.htaccess`, an Apache-like `.conf`) - a directive
  per line (**key**), a backslash continuing it; sections `<VirtualHost *:80>` ...
  `</VirtualHost>` with their tags as **keyword**; `%{HTTP_HOST}`, `%{ENV:X}`, `%1` and `$1`
  **function**; a line starting with `#` **comment**
- **OpenSSH** (`ssh_config`, `sshd_config`) - `Keyword value` or `Keyword=value` per line:
  `Host` and `Match` **keyword**, every other keyword **key**; `%h`, `%p`, `%%` and `${VAR}`
  **function**; a line starting with `#` **comment**
- **crontab** - the five time fields (`*/5`, `0-23`, `mon-fri`) **number**, `@reboot`,
  `@daily` ... **keyword**, `NAME=value` with the name as **key**; the command stays plain
- **hosts** - the address **number**, its names **key**, `#` a comment anywhere

In nginx, Apache and OpenSSH the values share their colours: quoted strings **string**, with the
variables in them in their own colour; numbers with a unit (`80`, `30d`, `10m`, `512k`) and
addresses (`127.0.0.1:8080`, `10.0.0.0/8`) **number**; `on off yes no true false none all any`
in any case **literal**. The plugin colours, it does not parse; a file that is not well formed is
coloured as far as it reads, never refused.

## Which files it is offered

The plugin claims files by their name (`[match] names`, RC-100-80), in any case, the name alone
and never the folder: the exact names `nginx.conf`, `httpd.conf`, `apache2.conf`, `.htaccess`,
`ssh_config`, `sshd_config`, `crontab` and `hosts`, asked before every other claim; the
extensions `.nginx` and `.htaccess`; and the patterns `*.nginx` and `*.conf`, asked after the
extensions. Names need rCommander 0.19.0.

A `.conf` without a name of its own - `sites-available/example.conf`, `conf.d/ssl.conf` - goes
first to the config viewer, whose claim is the extension `conf`. Its first line that is neither
blank nor a comment (`#`, `;`; in the first 4 KiB) decides: a block opening (`server {`,
`location / {`), a directive ending in `;` without `=` (`worker_processes auto;`) or a tag
(`<VirtualHost *:80>`, `</Directory>`) makes the config viewer decline it, and this plugin takes
it - as nginx, or as Apache for a tag or a line starting with an Apache directive (`LoadModule`,
`ServerRoot`, `RewriteEngine` ...). A section `[name]`, `key = value`, or a line of no known
shape stays with the config viewer, and this plugin declines it as well.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8, else `windows-1252`. `utf-8`, `utf-16le` and `windows-1252`
read the file in that encoding whatever it holds; a byte-order mark is skipped only when it is that
encoding's own. The decoding is `rc-plugin-text`'s, shared with the other text viewers. The text
the viewer shows is always UTF-8. A change on the Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - an nginx
statement or string going on, an Apache line continued. Colouring 50 lines in the middle of a
large file lexes from the nearest checkpoint; it does not read the file again. A line is
coloured up to 1 MiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/format.rs` - which grammar a file is read in: by its name, a `.conf` by its first line
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, and the values every grammar shares;
  `src/lexer/blocks.rs` nginx, Apache and OpenSSH; `src/lexer/tables.rs` crontabs and `hosts`
- `src/tokens.rs` - the tokens of the lines asked for, a line's text decoded
- `src/lines.rs` - the line index; `src/reader.rs` - the windows read and the lines of each
  encoding
- `manifest.toml` - identity, `[match]` (names and extensions), `[viewer] outputs`, limits and
  the `encoding` setting; the fields every bundled plugin shares are in `../metadata.toml`
- `locales/` - the plugin's name, description and the setting's label in English, German, French
  and Spanish
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (importing `settings`), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-server-config-viewer
cargo test -p rc-plugin-server-config-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh server-config-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it and runs the conformance check; the Windows package carries the
result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
