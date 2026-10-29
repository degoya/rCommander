# Contributing to rCommander

Thank you for taking the time. Bug reports, ideas, plugins and code are all welcome, and this page
explains how each of them reaches the project.

## How this repository works

This GitHub repository is an **export** of a private development repository. It is refreshed at
every release as one fresh commit, without the development history. That has one consequence
worth knowing before you open a pull request:

**A pull request here is a proposal, and it is closed rather than merged.** When a change is
accepted, the maintainer applies it by hand in the development repository and credits you in that
commit - as `Co-authored-by` with your GitHub noreply address, or however else you prefer; say so
in the pull request. The pull request is then closed with a pointer to the release that carries
your change. A closed pull request therefore does not mean a rejected one: the comment on it says
which it was.

Issues work as usual and are the best place to talk about a change before you write it.

## What helps most

- **Bug reports.** Use the bug report form. The version (Settings, page *About*), the Windows
  version, and the steps that lead to the problem make the difference between a fix and a guess.
- **Feature ideas.** Use the feature request form and describe the problem you want solved, not
  only the solution you have in mind. Total Commander is the functional reference: where
  rCommander does something differently from Total Commander, saying how Total Commander does it
  helps.
- **Plugins.** A viewer for another file format does not have to live in this repository. Plugins
  are signed WebAssembly components built with the SDK in [`sdk/`](sdk/); the handbook's
  [Plugin development](https://github.com/degoya/rCommander/wiki/plugin-development) page
  describes the manifest, the contract and how a package is signed and installed.

## Security issues

Please **do not** report a vulnerability in a public issue or pull request. Use GitHub's private
vulnerability reporting instead, as described in [SECURITY.md](SECURITY.md):
<https://github.com/degoya/rCommander/security/advisories/new>.

## Building and testing locally

The handbook's [Installation](https://github.com/degoya/rCommander/wiki/installation) page covers
the prerequisites and the cross-build for Windows from WSL2. `scripts/check.sh` runs the checks
that matter for your change - formatting, ShellCheck, the script tests, Clippy with
`-D warnings` and the Rust tests - and prints what it skipped and why;
[`scripts/README.md`](scripts/README.md) describes its modes. For a quick check while working,
`cargo nextest run -p <crate> <filter>`. Please say in your pull request which checks you ran.

## Conventions that matter in a pull request

These are the conventions a contribution most often runs into:

- **Rust sources are English-only** - comments, identifiers, test strings and commit messages.
  `crates/rcommander/tests/repo_lints/no_german.rs` fails on an umlaut or sharp s in a `.rs` file.
- **No `.unwrap()` outside tests.** Clippy warns on `unwrap_used` and the checks run with
  `-D warnings`; `unsafe_code`, `dbg!` and `todo!` are denied outright.
- **At most 500 lines per file**, Rust sources and scripts alike; a longer one is split into a
  module of its own. Unit tests live beside their code in a sibling `foo_tests.rs`.
- **UI strings are keys** in `crates/rcommander/locales/en.json`, never literals in a widget.
- **Every action is a command** of the registry in `rc-engine`, and every command either has an
  MCP tool in `rc-mcp` or a written reason why not (`crates/rc-mcp/mcp-coverage.md`, generated
  by `scripts/mcp-coverage.sh`).
- **Generated files are regenerated, never edited by hand**: `crates/rc-mcp/mcp-coverage.md`
  and the third-party licence list (`scripts/licenses.sh`).
- **Keep a change focused.** One fix or one feature per pull request is far easier to review and
  to carry over by hand than a mix. Add a line to `CHANGELOG.md` under `[Unreleased]` and update
  the documentation the change affects.
- **No secrets or personal data** in code, tests, fixtures or screenshots - no real host names,
  account names, keys or passwords.

## Code of Conduct

Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Licence

rCommander is licensed under the GNU General Public License v3.0 or later ([LICENSE](LICENSE)).
Contributions are accepted under the same licence: by submitting one, you agree that it is
licensed as GPL-3.0-or-later, like the rest of the project. There is no separate contributor
agreement and no sign-off requirement.
