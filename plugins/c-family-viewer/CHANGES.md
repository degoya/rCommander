# Changes

## 0.1.1

Built against plugin contract 0.4.0; nothing else changed.

## 0.1.0

The first release: source code of C (`.c`), C++ (`.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx`,
`.h`, `.ino`), C# (`.cs`, `.csx`), Java (`.java`), Kotlin (`.kt`, `.kts`), Go (`.go`) and Rust
(`.rs`), coloured like an editor by one lexer - the language follows the extension. Keywords,
built-in and declared types, functions called and defined, constants, numbers, comments,
operators and strings, with what each language adds: the C and C++ preprocessor and backslash
splices, C++ raw strings, C#'s verbatim, interpolated and raw strings and `#region`, Java's and
Kotlin's text blocks and annotations, Kotlin's string templates, Go's raw strings, Rust's
lifetimes, raw strings, attributes and macros, and nested comments in Rust and Kotlin. A string
the file or its line leaves open shows as invalid. The setting Encoding reads the file as UTF-8,
UTF-16 LE or Windows-1252; `auto` follows a byte-order mark, else takes UTF-8 when the file is
UTF-8, else Windows-1252.
