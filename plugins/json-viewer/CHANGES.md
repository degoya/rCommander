# Changes

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
