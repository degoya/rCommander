# Markup viewer

rCommander's bundled viewer plugin for reStructuredText, LaTeX and BibTeX, in one plugin with a
lexer per format (RC-100-83). It offers one view, **highlighted**: the text coloured like an
editor.

## Formats

The format is the file's extension, in any case; the content is never guessed from.

| Format | Extensions | What it reads |
| --- | --- | --- |
| reStructuredText | `.rst` | section titles - a line above an underline, so every line is lexed with the next beside it - overlines, transitions and table borders; explicit markup: directives (`.. note::`, `.. code-block:: python`) with their options and indented content, targets `.. _name: url`, footnotes and citations `.. [1]`, substitution definitions `.. \|name\| image::`, comments with their indented block; field lists `:Author:`; bullets, enumerators (`1.`, `#.`, `(a)`, `iv.`), line blocks; doctests `>>>`; a paragraph ending in `::` and the literal block after it; inline markup: `**strong**`, `*emphasis*`, ``` ``literal`` ```, `` `text` `` with a role before or after it (`:ref:`, `:py:func:`), references `` `text <url>`_ ``, `name_`, `name__`, `\|substitution\|`, `[1]_`, and addresses `https://...` |
| LaTeX | `.tex`, `.sty` | commands (`@` a letter, as in a package) and control symbols (`\\`, `\%`); `%` comments; braces and brackets; math in `$...$`, `$$...$$`, `\(...\)`, `\[...\]` over lines - its text a string, its commands keywords - which a blank line ends, as TeX does; the environment of `\begin` and `\end`; packages and classes; the keys of `\label`, `\ref`, `\cite` and their kin, past an optional argument; files of `\input`, `\include`, `\includegraphics`; the command `\newcommand`, `\def`, `\let` define; `#1`; `&`, `~`, `^`, `_`; the environments `verbatim`, `Verbatim`, `lstlisting`, `minted` and `comment` up to their `\end`; `\verb\|...\|` |
| BibTeX | `.bib` | entries `@type{key, field = value}` or with parentheses: the type, the citation key (also on the next line), fields, values in braces (nested) or quotes over lines, numbers, abbreviations (`jan`, an `@string`'s name) and `#` joining them; `@string` and `@preamble` without a key; `@comment`, and every text between entries a comment, as BibTeX ignores it |

## Classes

The formats are no code, so a class names a role:

| Class | reStructuredText | LaTeX | BibTeX |
| --- | --- | --- | --- |
| **keyword** | section titles, directive names, `**strong**` | commands | entry types `@article` |
| **key** | fields and options, targets, substitutions, footnote and citation labels | labels, references and citation keys, `#1` | field names |
| **type-name** | references and addresses, a code block's language | environments, packages and classes | citation keys |
| **function** | roles | the command a definition names | - |
| **literal** | `*emphasis*` | - | abbreviations |
| **string** | literals, literal and code blocks, interpreted text, doctests | math, verbatim text, `\verb`, files | values |
| **number** | enumerators | - | numbers |
| **comment** | comments | `%` comments, the `comment` environment | text between entries, `@comment` |
| **operator**, **punctuation** | bullets, `>>>`; adornments, borders, `..`, `::` | math delimiters, control symbols, `& ~ ^ _`; braces | `=`, `#`; braces, commas |
| **invalid** | - | math a blank line ends or the file leaves open, a verbatim environment left open | a value the file leaves open |

What reaches from one line into the next: an indented reStructuredText block and the literal
block a `::` promises, whether a line underlines a title; LaTeX math and verbatim environments;
a BibTeX entry, its key still to come, and a value over lines. Inline markup over lines is not
followed: markup the line does not close stays text; an end-string is looked for at most 4096
bytes on.

## Encoding

The setting **Encoding** (`encoding`) says how the file's bytes are read: `auto`, the default,
follows a byte-order mark (UTF-8, UTF-16 LE or BE), else takes UTF-8 when the start of the file
(its first 1 MiB) is valid UTF-8, else `windows-1252`, for an old document saved in the ANSI
code page. `utf-8`, `utf-16le` and `windows-1252` read the file in that encoding whatever it
holds; a byte-order mark is skipped only when it is that encoding's own. The decoding is
`rc-plugin-text`'s, shared with the other text viewers. The text the viewer shows is always
UTF-8 - search, copy and the token colours work on it. A change on the Plugins page holds from
the next file opened.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps only an index of at most
16 MiB: a checkpoint every 64 lines (or 64 KiB) with the lexer's state there, and the openings
left open. Colouring 50 lines in the middle of a 100 MiB file lexes from the nearest checkpoint,
reading one line ahead; it does not read the file again. The lexers never go back and look
ahead a bounded distance only, so their time is linear in the file. A line is coloured up to
1 MiB and 4096 tokens; the rest stays plain.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`), `probe` and the setting, with the host's
  promises at the top
- `src/lang.rs` - `Lang`: the extensions
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/lexer.rs` - the state a line starts in, the openings `open` sees left open, the
  dispatch; `src/lexer/rst.rs` reStructuredText's lines and blocks, `src/lexer/inline.rs` its
  inline markup, `src/lexer/latex.rs`, `src/lexer/bibtex.rs`, `src/lexer/cursor.rs` the cursor
- `src/tokens.rs` - the tokens of the lines asked for, a line's text decoded, the next line read
  ahead
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
cargo test -p rc-plugin-markup-viewer
cargo test -p rc-plugin-markup-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh markup-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain: a sample per format, and a 5 MiB file
within the manifest's time limit. `scripts/build-plugins.sh` builds the component for
`wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
