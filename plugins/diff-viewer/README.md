# Diff viewer

rCommander's bundled viewer plugin for diffs and patches (RC-100-24). It offers two views:
**highlighted**, the patch coloured, shown first, and **tree**, its files and hunks to unfold.

## Highlighted

The colours are the theme's token classes, light and dark - no class of its own:

- **number** (green) - an added line: `+...`, `++`/`+ ` in a combined diff, `+ ` and the new
  side's `! ` in a context diff
- **invalid** (red) - a removed line: `-...`, a `-` in any column of a combined diff, `- ` and
  the old side's `! ` in a context diff
- **keyword** - a hunk header `@@ -a,b +c,d @@` (`@@@ ... @@@` combined), a context diff's
  `***************`, `*** a,b ****` and `--- c,d ----`
- **function** - the function context git writes after a hunk header
- **type-name** - `diff ...`, `index`, `---`, `+++`, `***`, `old mode`/`new mode`,
  `new file mode`, `deleted file mode`, `similarity index`, `rename from`/`rename to`,
  `copy from`/`copy to`, `Binary files ... differ`, `GIT binary patch`
- **comment** - `\ No newline at end of file`, and the text outside the diff: the mail header
  and message of `git format-patch` before the first diff, its signature after the last
- an unchanged line stays uncoloured

A unified or combined hunk is **counted**, not guessed: its header promises so many lines, and
the hunk ends when they are read. A removed line `--- x` (an SQL comment taken out) stays a
removed line inside its hunk; the same text after the hunk starts the next file. A hunk header
without counts colours its body as long as lines start like one. A context diff is followed by
its markers alone.

## Tree

A node per file: the path as the label, `+added -removed` as the value, followed by `new`,
`deleted`, `binary`, `renamed from <old path>` or `copied from <old path>` when git's headers
say so. Under a file a node per hunk: its header as the label, its line count as the value.
The path comes from `+++` (or the context diff's `---`), from `---` when the file was deleted,
from `rename to`/`copy to`, else from the `diff` line; git's `a/` and `b/` are left out.

More than 1000 files, or hunks of one file, are listed as ranges of 1000. At most 32768 files
and 131072 hunks are kept; a last node, **not listed**, counts the rest.

The text is the file's own, so the plugin has no settings. Bytes that are not UTF-8 show as
U+FFFD. It needs no capability: it reads the file it is handed and nothing else.

## Which files it is offered

rCommander decides from the manifest's `[match]` without running the plugin, then asks `probe`:

- **By extension**: `.diff`, `.patch`, `.rej` - any text is taken.
- **By how it starts**, for a file without one of those: `diff ` (`git diff`, `diff -u` with
  its command line, `diff -r`), `--- ` (a plain `diff -u`) or `From ` - and `probe` then wants
  `--- ` followed by a `+++ ` line, and `From ` by a 40-digit commit id and a blank, as
  `git format-patch` writes it; a mail that only starts with `From ` goes to the built-in viewer.
- **After the extensions**: rCommander asks the plugins whose extension matches before those
  whose magic does, and a declining `probe` hands the file to the next - a YAML document starting
  `--- ` stays with the YAML viewer.
- **`probe`** declines a binary file (a NUL in the first 4 KiB) in any case.

## Large files

`open` reads the file once, in windows of at most 1 MiB, and keeps an index and the tree of at
most 8 MiB together: a checkpoint every 64 lines (or 64 KiB) with the state there - before the
first diff, between files, how many lines a hunk still promises from its parents and for the
result, which side of a context hunk - and per file and hunk where its name is in the file,
never the name itself. Colouring 50 lines in the middle of a 100 MiB patch classifies from the
nearest checkpoint and never reads the file again. A line is read and coloured up to 64 KiB;
it gets two tokens at most (the host allows 4096).

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`, `tokens`, `children`) and `probe`, with the host's
  promises at the top
- `src/types.rs` - the viewer world's records and the `source`/`sink` interfaces, mirrored one
  to one
- `src/classify.rs` - what each line is, and the state the next one starts in
- `src/tokens.rs` - the tokens of the lines asked for
- `src/tree.rs`, `src/tree/paths.rs` - the files and hunks, and where a header names a path
- `src/lines.rs` - the line index
- `src/reader.rs`, `src/utf8.rs` - the lines read through windows, and the UTF-8 the text is
  written in
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits; the fields every bundled
  plugin shares are in `../metadata.toml`
- `locales/en.json` - the plugin's name and description
- `CHANGES.md` - the release notes, a `## <version>` section per version
- `src/guest.rs` - the translation into the viewer world of `crates/rc-plugin-api/wit`
  (`rcommander:plugin@0.2.0`; the `settings` import is never called, so the component does not
  import it), only for `wasm32`

## Test and build

```bash
cargo test -p rc-plugin-diff-viewer
cargo test -p rc-plugin-diff-viewer --release -- --ignored   # the 100 MiB case
scripts/build-plugins.sh diff-viewer                          # component, signed package
```

The tests run natively, without a WebAssembly toolchain. `scripts/build-plugins.sh` builds the
component for `wasm32-unknown-unknown`, checks its imports, packages it with the shared fields of
`../metadata.toml`, signs it with the development key and runs the conformance check; the
Windows package carries the result in `plugins/`, and rCommander installs it at its first start.

Licence: MIT.
