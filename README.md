# rCommander

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="crates/rcommander/resources/brand/rCommander-logo-dark.svg">
  <img src="crates/rcommander/resources/brand/rCommander-logo.svg" alt="rCommander" width="480">
</picture>

A two-panel file manager in the tradition of Total Commander and Double Commander, written in
Rust.

- Two panels with tabs; each panel shows either the file list or a terminal. The lists offer a
  folder tree, thumbnails, git status, your own column sets, and refresh themselves when a
  folder changes.
- WSL distributions, SSH servers and phones or cameras on USB (MTP, Windows and Linux) in the
  drive bar, next to the local drives.
- A small SSH connection manager: servers, keys, credentials, login scripts and one-time codes
  for two-factor logins, stored encrypted behind an optional master password; inventory plugins
  keep connections in step with NetBox, Proxmox or another system.
- Copying local to remote and remote to remote over SFTP and FTP/FTPS, with Total Commander's
  rename masks; multi-rename; files on a server open for editing through a local copy whose
  saves go back up. SSH connections can go through jump hosts, forward ports and log in with
  OpenSSH user certificates.
- Your own commands, a Start menu, button bars with icons (and Total Commander's .bar files) and internal
  associations; find files inside archives and through Everything, duplicate files and a disk
  usage view; archives changed in place.
- A viewer (Lister and Quick View) whose bundled plugins colour JSON (with JSONC, JSON5 and
  JSON Lines), XML, YAML, CSV, SQL, configuration files, shell, PowerShell, batch and Python
  scripts, Dockerfiles, Makefiles, server configuration (nginx, Apache, SSH), diffs, logs, web code, SVG,
  C-like languages from C to Rust, Terraform/HCL, Protocol Buffers, GraphQL, Ruby, Perl, Lua,
  VBScript, reStructuredText, LaTeX, BibTeX, Visual Studio solutions and to-do lists - chosen by
  the file's extension or its whole name - and show Office documents, the text of PDFs and the
  details of pictures, audio and video. Built in: certificates and keys with their checks (no
  private key is ever shown), coloured .env and other credential files that no plugin sees, and
  a Markdown preview with coloured code.
- Colour themes for the whole window and the terminal, built in or your own, switched from the
  command palette or a key, and a theme per connection or WSL distribution so each server's
  panel and terminal stand out.
- Workspaces: both panels' tabs, servers and terminals saved under a name and brought back in
  one step.
- A portable mode that keeps all data beside the program.
- Every action is also available to AI agents through a built-in MCP server.
- In English, German, French and Spanish.

Website: <https://rcommander.net>

## Status

Early development: expect rough edges and changes between releases. What each release changed
is in [CHANGELOG.md](CHANGELOG.md). Every release ships for Windows, Linux and macOS.

## Download

Every release is on [GitHub Releases](https://github.com/degoya/rCommander/releases), and the
current one on [rcommander.net](https://rcommander.net). No installer and no administrator rights
are needed:

- **Windows x64**: unpack the zip anywhere and start `rcommander.exe`.
- **Linux x64 and arm64** (glibc 2.39 or newer, e.g. Ubuntu 24.04): unpack the tarball and start
  `./rcommander`.
- **macOS, Apple Silicon and Intel**: unzip it and move `rCommander.app` to *Applications*. The
  app is not notarised, so open it the first time with a right-click and *Open*.

Every release lists its files with their SHA-256 in `SHA256SUMS`, and carries a `.deb` and an
`.rpm` package for each Linux architecture. An unpacked package updates itself: by default a new
version installs when you quit rCommander (Settings > Updates).

To carry rCommander on a USB stick with its settings, connections and plugins, unpack the Windows
zip or the Linux tarball there and choose *Make this copy portable...* in the database dialog
(Configuration > Database...): from the next start the data lives in `data` beside the program.

### Package managers

```bash
brew install --cask degoya/rcommander/rcommander   # macOS (Homebrew)
brew install degoya/rcommander/rcommander          # Linux (Homebrew)
scoop bucket add rcommander https://github.com/degoya/scoop-rcommander
scoop install rcommander                           # Windows (Scoop)
winget install degoya.rCommander                   # Windows (winget)
```

winget lists rCommander once Microsoft has reviewed its first submission.

Debian, Ubuntu and Fedora install from the signed apt and dnf repository; its page
<https://degoya.github.io/rcommander-packages> shows the two files to add. An installation through
a package manager is updated by it (`brew upgrade`, `scoop update`, `winget upgrade`,
`apt upgrade`, `dnf upgrade`); rCommander's update window then shows that command instead of
installing the update itself.

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

## Thanks

- [dogsventuring-sascha](https://github.com/dogsventuring-sascha) contributed the macOS look: the
  menus in the system's menu bar, Cmd as the shortcut key, the Mac's colours, accent and fonts
  and the transparent title bar.

## Licence

GPL-3.0-or-later, see [LICENSE](LICENSE).
