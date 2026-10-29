# Changes

## 0.1.2

Built against plugin contract 0.3.0; nothing else changed.

## 0.1.1

A diff without one of its extensions is also found when it starts with `--- ` followed by a
`+++ ` line, as a plain `diff -u` writes it. A YAML document starting with `--- ` stays with the
YAML viewer, which rCommander now asks first.

## 0.1.0

The first release: unified, combined and context diffs coloured - added lines green, removed
lines red, file and hunk headers marked, a `git format-patch` mail header as a comment - and the
patch's files with their hunks as a tree. Found as `.diff`, `.patch` and `.rej`, or by a start
of `diff ` or `From <commit>`.
