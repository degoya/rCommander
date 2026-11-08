# Changes

## 0.1.2

Rebuilt against plugin contract 0.6.0; no other change.

## 0.1.1

Built against plugin contract 0.4.0; nothing else changed.

## 0.1.0

The first release: Makefiles (`Makefile`, `makefile`, `GNUmakefile` and `.mk`, claimed by their
name) coloured like an editor - targets and special targets, assignments of every flavour (`=`,
`:=`, `::=`, `?=`, `+=`, `!=`), references `$(...)` and `${...}`, the automatic variables, make's
functions with their arguments, the directives (`include`, `ifeq`, `define` ...), `define` bodies
up to `endef`, order-only prerequisites and static patterns, comments, and recipe lines with the
shell's keywords, builtins, strings and operators. A line a backslash continues goes on as what it
was. The setting Encoding reads the file as UTF-8, UTF-16 LE or Windows-1252; `auto` follows a
byte-order mark, else takes UTF-8 when the file is UTF-8, else Windows-1252. Needs rCommander
0.19.0, which claims files by their name.
