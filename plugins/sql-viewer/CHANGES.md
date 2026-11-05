# Changes

## 0.1.0

The first release: SQL scripts (`.sql`, `.ddl`, `.dml`, `.psql`, `.pgsql`, `.mysql`) coloured
like an editor - the keywords of ANSI SQL, PostgreSQL, MySQL, SQLite and T-SQL in any case, the
built-in types, `TRUE`, `FALSE` and `NULL`, functions called, strings with `''` and the `N'`,
`E'`, `X'` and `B'` prefixes, names in `"..."`, `` `...` `` and `[...]`, variables and
parameters (`@id`, `@@version`, `$1`, `:name`), numbers, operators, `--` and nested `/* */`
comments, and PostgreSQL's dollar-quoted bodies (`$$ ... $$`, `$body$ ... $body$`) over lines. A
`.mysql` file and a MySQL or MariaDB dump also read `#` comments, `"..."` strings and backslash
escapes. The setting Encoding reads the file as UTF-8, UTF-16 LE or Windows-1252; `auto` follows
a byte-order mark, else takes UTF-8 when the file is UTF-8, else Windows-1252.
