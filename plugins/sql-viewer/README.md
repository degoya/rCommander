# SQL viewer

rCommander's bundled viewer plugin for SQL scripts: `.sql`, `.ddl`, `.dml`, `.psql`, `.pgsql`
and `.mysql` (RC-100-81). It offers one view, **highlighted**: the script coloured like an editor.

- **keyword** - one table for every dialect, in any case: ANSI SQL (`SELECT`, `INSERT`,
  `CREATE`, `JOIN`, `WHERE`, `GROUP BY`, ...), PostgreSQL (`RETURNING`, `ILIKE`, `LANGUAGE`,
  `PERFORM`, `RAISE`, ...), MySQL (`AUTO_INCREMENT`, `ENGINE`, `DELIMITER`, ...), SQLite
  (`PRAGMA`, `AUTOINCREMENT`, `GLOB`, ...) and T-SQL (`TOP`, `GO`, `NOCOUNT`, `RAISERROR`,
  `TRY`, `CATCH`, ...)
- **type-name** - the built-in types (`INT`, `VARCHAR`, `TEXT`, `NUMERIC`, `TIMESTAMPTZ`,
  `JSONB`, `UNIQUEIDENTIFIER`, `NVARCHAR`, `BLOB`, ...)
- **literal** - `TRUE`, `FALSE`, `NULL`, `UNKNOWN`
- **function** - a name a bracket follows right away: `count(`, `now(`, `coalesce(`
- **string** - `'...'` with `''`, the prefixes `N'`, `E'` (with backslash escapes), `X'` and
  `B'`, and PostgreSQL's dollar-quoted bodies `$$ ... $$` and `$tag$ ... $tag$`; a string and a
  body go on over lines
- **key** - names in `"..."` (over lines), `` `...` `` and T-SQL's `[...]` (`[dbo].[Orders]`;
  not after a name, where `[` is a subscript), and variables and parameters: `@id`,
  `@@version`, `$1`, `:name`
- **number** - `42`, `1.5`, `.5`, `6.02e23`, `0x1F`; **comment** - `-- ...` and `/* ... */`
  over lines, nested as PostgreSQL nests them; **operator** - `= <> != <= || :: ->> @>` and the
  others; **punctuation** - brackets, `,`, `;`, `.`
- a name of no kind - a table, a column, `#temp` - stays uncoloured

A word after `.` is a name: `t.date` colours no type. The plugin colours, it does not parse: there
is no dialect parser, a word that is a keyword in one dialect is one in every file, and a script
that is not well formed is coloured as far as it reads, never refused.

**MySQL** reads text differently: `#` starts a comment, `"..."` is a string and a backslash
escapes the next character in a string. The viewer reads a file that way when its extension is
`.mysql` or its first line is a `mysqldump` or `mariadb-dump` header (`-- MySQL dump ...`,
`-- MariaDB dump ...`); every other file is read the standard way.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE - SQL Server Management Studio saves UTF-16 LE
with one), else takes UTF-8 when the start of the file (its first 1 MiB) is valid UTF-8, else
`windows-1252`, for a script saved in the ANSI code page. `utf-8`, `utf-16le` and `windows-1252`
read the file in that encoding whatever it holds; a byte-order mark is skipped only when it is that
encoding's own. The decoding is `rc-plugin-text`'s, shared with the other text viewers. The text
the viewer shows is always UTF-8 - search, copy and the token colours work on it. A change on the
Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - a block comment, a
string, a quoted name or a dollar-quoted body still open. Colouring 50 lines in the middle of a
100 MiB dump lexes from the nearest checkpoint; it does not read the file again. A line is
coloured up to 1 MiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe`, the setting and the MySQL
  reading, with the host's promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, comments, strings, names and bodies;
  `src/lexer/code.rs` words, numbers, operators, variables and what opens a string;
  `src/lexer/words.rs` the tables of keywords, types and literals
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
cargo test -p rc-plugin-sql-viewer
cargo test -p rc-plugin-sql-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh sql-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it and runs the conformance check; the Windows package carries the
result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
