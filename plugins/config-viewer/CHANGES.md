# Changes

## 0.2.0

More files: `.env.local`, `.env.production` and every other `.env.*` (rCommander 0.19.0 and
later), systemd units (`.service`, `.timer`, `.socket`, `.mount`, `.path`, `.target`),
`.gitmodules` and `.npmrc` as INI, and `.dockerignore` like `.gitignore`.
A `.conf` that reads like nginx or Apache (a `{` block, a `directive;`, a `<Tag>` first) is left
to the server configuration viewer.

## 0.1.1

Built against plugin contract 0.3.0; nothing else changed.

## 0.1.0

The first release: configuration files coloured like an editor or unfolded as a tree - INI and
its kin (`.ini`, `.inf`, `.cfg`, `.conf`, `.editorconfig`, `.gitconfig`), Windows registry
exports (`.reg`), `.env`, Java `.properties`, TOML (`Cargo.lock` too), `.gitignore` and
`.gitattributes`. The setting Encoding reads the file as UTF-8, UTF-16 LE or Windows-1252; `auto`
takes a byte-order mark's encoding, else UTF-8 when the file starts as UTF-8, else Windows-1252.
