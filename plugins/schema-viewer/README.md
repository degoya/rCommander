# Schema viewer

rCommander's bundled viewer plugin for schema languages - Protocol Buffers and GraphQL - in one
plugin (RC-100-83). Both describe keywords, types, fields and blocks, which is why one plugin
reads both. It offers one view, **highlighted**: the file coloured like an editor.

## Languages

The language is the file's extension, in any case; the content is never guessed from.

| Language | Extensions |
| --- | --- |
| Protocol Buffers | `.proto` |
| GraphQL | `.graphql` `.gql` `.graphqls` |

## Classes

**Protocol Buffers**

- **keyword** - `syntax`, `edition`, `package`, `import`, `message`, `enum`, `service`, `rpc`,
  `returns`, `stream`, `oneof`, `map`, `repeated`, `optional`, `reserved`, `option`, ... (proto2,
  proto3 and editions)
- **type-name** - the scalar types (`int32`, `string`, `bytes`, ...), a declared message, enum or
  service, a field's type - also dotted (`google.protobuf.Timestamp`) - the types of
  `map<K, V>` and of an `rpc`'s brackets
- **function** - an `rpc`'s name
- **key** - a name `=` or `:` follows: a field's, an enum value's, an option's
  (`deprecated = true`, `get: "/v1/..."`)
- **literal** - `true`, `false`, `inf`, `nan`
- **string** - `"..."` and `'...'` with backslash escapes; **number** - `42`, `0x1F`, `1.5e3`;
  **comment** - `// ...` and `/* ... */` over lines

**GraphQL**

- **keyword** - `type`, `interface`, `union`, `enum`, `input`, `scalar`, `schema`, `extend`,
  `implements`, `directive`, `repeatable`, `query`, `mutation`, `subscription`, `fragment`, `on`
- **type-name** - the built-in scalars (`Int`, `Float`, `String`, `Boolean`, `ID`), a declared
  type, the types after `implements`, `&`, `on` and a union's `=` and `|`, and every name in
  `PascalCase` (`Episode`); an enum value in capitals (`JEDI`) stays uncoloured
- **function** - an operation's and a fragment's name, a fragment spread (`...HeroFields`), a
  field with arguments (`hero(`), a directive (`@include`, `@deprecated`)
- **key** - a name `:` follows - a field, an argument, an alias - and a variable (`$episode`)
- **literal** - `true`, `false`, `null`
- **string** - `"..."` with backslash escapes, and block strings `"""..."""` over lines (`\"""`
  is text); **number** - `42`, `-7`, `1.5e3`; **comment** - `# ...`

Both: **operator** - `=`, `-`, `+`, GraphQL's `!`, `&`, `|` and `...`; **punctuation** -
brackets, `,`, `;`, `:`, `.`, `<`, `>`; **invalid** - the opening quote of a `"..."` string its
line leaves open, which both languages refuse. A name of no kind stays uncoloured.

The plugin colours, it does not parse: a file that is not well formed is coloured as far as it
reads, never refused.

### Why its own lexer

The C-family viewer's lexer (RC-100-82) is not shared: GraphQL is no C-like language (`#`
comments, no `/* */`, block strings with one escape), and of the C lexer's machinery - the
preprocessor, raw, verbatim and interpolated strings, splices, nested comments - Protobuf needs
none. Sharing it would have meant turning its closed set of seven languages into an open
description and moving it into a library crate, a change to the C-family viewer for the few
dozen lines Protobuf's comments, strings and numbers take here.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8 - what both languages' specifications demand - else
`windows-1252`, for a file saved in the ANSI code page. `utf-8`, `utf-16le` and `windows-1252`
read the file in that encoding whatever it holds; a byte-order mark is skipped only when it is that
encoding's own. The decoding is `rc-plugin-text`'s, shared with the other text viewers. The text
the viewer shows is always UTF-8 - search, copy and the token colours work on it. A change on the
Plugins page holds from the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there - a block comment or
a block string still open. Colouring 50 lines in the middle of a 100 MiB file lexes from the
nearest checkpoint; it does not read the file again. A line is coloured up to 1 MiB and 4096
tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top; `src/lang.rs` - the language by the extension
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the lexer, the state a line starts in, block comments, block strings, strings
  and numbers; `src/lexer/proto.rs` and `src/lexer/graphql.rs` each language's code and names;
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
cargo test -p rc-plugin-schema-viewer
cargo test -p rc-plugin-schema-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh schema-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it and runs the conformance check; the Windows package carries the
result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
