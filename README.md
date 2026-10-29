# rCommander

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="crates/rcommander/resources/brand/rCommander-logo-dark.svg">
  <img src="crates/rcommander/resources/brand/rCommander-logo.svg" alt="rCommander" width="480">
</picture>

A two-panel file manager in the tradition of Total Commander and Double Commander, written in
Rust.

- Two panels with tabs; each panel shows either the file list or a terminal.
- WSL distributions and SSH servers in the drive bar, next to the local drives.
- A small SSH connection manager: servers, keys, credentials and login scripts, stored
  encrypted behind an optional master password.
- Copying local to remote and remote to remote over SFTP and FTP/FTPS; multi-rename.
- Every action is also available to AI agents through a built-in MCP server.

Website: <https://rcommander.net>

## Status

Early development: expect rough edges and changes between releases. The plan is in
[ROADMAP.md](ROADMAP.md), what each release changed in [CHANGELOG.md](CHANGELOG.md). Windows
comes first; Linux and macOS follow.

## Download

Every release is on [GitHub Releases](https://github.com/degoya/rCommander/releases), and the
current one on [rcommander.net](https://rcommander.net). On Windows, unpack the zip anywhere and
start `rcommander.exe`; no installer and no administrator rights are needed.

## Documentation

The handbook is the [wiki](https://github.com/degoya/rCommander/wiki): installation, first steps,
the panels, connections, the terminal, plugins, the keyboard shortcuts and the MCP server.

## Building from source

```bash
git clone https://github.com/degoya/rCommander.git
cd rCommander
cargo build --release --locked
```

The pinned Rust toolchain installs itself on the first build (`rust-toolchain.toml`). The
handbook's [Installation](https://github.com/degoya/rCommander/wiki/installation) page covers the
cross-build for Windows from WSL2; [scripts/README.md](scripts/README.md) lists the checks and
packaging scripts.

## Contributing

Bug reports and ideas are welcome as issues. This repository is an export of the development
repository, refreshed at every release: a pull request is a proposal that is applied by hand,
with credit. [CONTRIBUTING.md](CONTRIBUTING.md) explains how, and [SECURITY.md](SECURITY.md) how
to report a vulnerability privately.

## Licence

GPL-3.0-or-later, see [LICENSE](LICENSE).
