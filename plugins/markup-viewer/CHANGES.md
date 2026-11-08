# Changes

## 0.2.0

To-do lists in TaskPaper's and PlainTasks' plain text (`.todo`, `.taskpaper`): projects (a line
ending in `:`), tasks by their mark - open (`-`, `*`, `☐`, `□`, `❍`, `❑`, `[ ]`), done (`✔`,
`✓`, `☑`, `+`, `[x]`) or cancelled (`✘`, `✗`) - and tags `@name` and `@name(value)`. The date
tags (`@done`, `@started`, `@due`, `@created`, `@cancelled`, `@lasted`, `@wasted`,
`@estimated`, `@today`) have a colour of their own, and a value that starts with a digit - the
date and time of `@done(26-05-10 14:15)` - another than a word. A task done or cancelled, by its
mark or by `@done` or `@cancelled`, shows dimmed with its tags still coloured; PlainTasks'
archive rule is a comment.

## 0.1.1

Rebuilt against plugin contract 0.6.0; no other change.

## 0.1.0

The first release: reStructuredText (`.rst`), LaTeX (`.tex`, `.sty`) and BibTeX (`.bib`),
coloured like an editor by a lexer per format - the format follows the extension. In
reStructuredText section titles (known by the underline after them), adornments and table
borders, directives with their options and content, targets, footnotes, substitutions,
comments, fields, bullets and enumerators, literal blocks after `::`, doctests, and the inline
markup: strong, emphasis, literals, roles, references and addresses. In LaTeX commands,
comments, math with its delimiters, environments, packages, labels, references and citations,
files, the command a definition names, `#1`, verbatim environments and `\verb`. In BibTeX entry
types, citation keys, fields, values in braces or quotes over lines, numbers, abbreviations and
the text between entries as a comment. Math a blank line ends, or a value or verbatim
environment the file leaves open, shows its opening as invalid. The setting Encoding reads the
file as UTF-8, UTF-16 LE or Windows-1252; `auto` follows a byte-order mark, else takes UTF-8 when
the file is UTF-8, else Windows-1252.
