# Changes

## 0.1.1

Built against plugin contract 0.3.0; nothing else changed.

## 0.1.0

The first release: log files (`.log`, `.out`, `.err`) coloured so the errors stand out - levels
(error red, warning yellow, information, debug muted), timestamps, IP addresses, URLs and paths,
`key=value` and JSON keys, quoted strings, and Java, .NET, JavaScript and Python stack traces.
The setting Level colour colours the level word or the whole line of an error or a warning with
its stack trace; the setting Encoding reads the file as UTF-8, UTF-16 LE or Windows-1252, `auto`
by its byte-order mark, else UTF-8 when the file is UTF-8, else Windows-1252. Logs of hundreds of
MiB open in one pass with a bounded index.
