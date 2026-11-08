# Changes

## 0.1.1

Rebuilt against plugin contract 0.6.0; no other change.

## 0.1.0

The first release: scripts in Ruby (`.rb`, `Gemfile`, `Rakefile`), Perl (`.pl`, `.pm`), Lua
(`.lua`) and VBScript (`.vbs`), coloured like an editor by a lexer per language - the language
follows the file's name. Keywords, constants, well-known functions and the names `def`, `sub`,
`function`, `Sub`, `Function` and `Class` declare, variables, numbers, comments, operators and
strings, with what each language adds: Ruby's `=begin` blocks, `%w[...]` and the other `%`
literals, symbols, hash keys and interpolation (`#{...}`, `#@name`); Perl's POD, quote-like
operators (`q`, `qq`, `qw`, `m`, `s`, `tr`) with any delimiter, sigils and interpolated
variables; Ruby's and Perl's heredocs and the data after `__END__`; Lua's long strings and
comments (`[==[ ... ]==]`) and labels; VBScript's `Rem`, `""` in strings and `#dates#`. A string
the file or its line leaves open shows as invalid. The setting Encoding reads the file as UTF-8,
UTF-16 LE or Windows-1252; `auto` follows a byte-order mark, else takes UTF-8 when the file is
UTF-8, else Windows-1252.
