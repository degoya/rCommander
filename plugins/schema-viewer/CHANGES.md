# Changes

## 0.1.0

The first release: Protocol Buffers (`.proto`) and GraphQL (`.graphql`, `.gql`, `.graphqls`)
schemas coloured like an editor, the language by the extension. Protobuf: keywords of proto2,
proto3 and editions, scalar types, declared messages, enums and services, `rpc` names and the
message types of their signatures, field types (also dotted, `google.protobuf.Timestamp`) and
`map<K, V>`, field and option names, `true`, `false`, `inf`, `nan`, numbers, `"..."` and
`'...'` strings, `//` and `/* */` comments. GraphQL: keywords, built-in scalars, declared and
`PascalCase` types, operation and fragment names, fragment spreads, fields, arguments and
aliases, fields with arguments, `$variables`, `@directives`, `true`, `false`, `null`, numbers,
`"..."` and block strings `"""..."""` over lines, `#` comments. A string its line leaves open
shows its opening quote as invalid. The setting Encoding reads the file as UTF-8, UTF-16 LE or
Windows-1252; `auto` follows a byte-order mark, else takes UTF-8 when the file is UTF-8, else
Windows-1252.
