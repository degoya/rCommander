# Changes

## 0.1.0

The first release: Python scripts and type stubs (`.py`, `.pyw`, `.pyi`), and scripts without an
extension whose first line runs Python, coloured like an editor - keywords and the soft keywords
`match`, `case` and `type` where they head a statement, decorators, the names of functions and
classes defined, built-in functions, types and exceptions, `True`, `False` and `None`, numbers,
comments, operators, and strings with every prefix: raw, bytes, f-strings and t-strings with the
expressions in their fields, also over several lines. A string the file or its line leaves open
shows as invalid. The setting Encoding reads the file as UTF-8, UTF-16 LE or Windows-1252; `auto`
follows a byte-order mark, else takes UTF-8 when the file is UTF-8, else Windows-1252.
