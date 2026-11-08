# Changes

## 0.3.2

Rebuilt against plugin contract 0.6.0; no other change.

## 0.3.1

Built against plugin contract 0.4.0; nothing else changed.

## 0.3.0

Reads three more kinds of JSON, chosen by the file's extension:

- `.jsonc` - JSON with comments: `//` and `/* */` comments and a comma before a closing bracket.
- `.json5` - JSON5: comments and such commas, keys without quotes, strings in single quotes,
  hexadecimal numbers, `+1`, `.5`, `5.`, `Infinity` and `NaN`.
- `.jsonl` and `.ndjson` - a value per line: each line is checked for itself, so a broken line
  is marked as an error and the lines after it read as usual; the tree lists the lines' values.

Comments are coloured as comments. The setting Format `pretty` lays out the values without
their comments. A comma before a closing bracket no longer leaves an empty line in `pretty`.

## 0.2.2

Built against plugin contract 0.3.0; nothing else changed.

## 0.2.1

A file that is not valid UTF-8 shows a replacement character for each broken sequence, and the
colours after it stay on their characters: before, the colours of such a line moved or were
dropped.

## 0.2.0

The setting Format works: `pretty` shows the JSON indented two spaces a level, so a minified
file reads line by line; `as_is`, the default, shows the file as it is. Built against the plugin
contract 0.2.0, which hands a plugin its settings.

## 0.1.0

The first release: JSON coloured like an editor or unfolded as a tree, with the text as the file
has it.
