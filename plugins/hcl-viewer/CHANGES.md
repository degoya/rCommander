# Changes

## 0.1.0

The first release: Terraform (`.tf`, `.tfvars`) and HCL (`.hcl`) files coloured like an editor -
block types and their labels, attribute names, quoted templates with `${ ... }` interpolations
and `%{ ... }` directives coloured as code inside, heredocs (`<<EOT`, `<<-EOT`) over lines with
their markers, `for` expressions, function calls, type constraints, `true`, `false` and `null`,
numbers, operators, and `#`, `//` and `/* */` comments. A quoted template its line leaves open
shows its opening quote as invalid. The setting Encoding reads the file as UTF-8, UTF-16 LE or
Windows-1252; `auto` follows a byte-order mark, else takes UTF-8 when the file is UTF-8, else
Windows-1252.
