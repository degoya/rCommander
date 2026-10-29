# Writing an rCommander plugin

Copied and adapted from rDownloader's `sdk/README.md` at commit 26c8592a6.

A plugin is a signed WebAssembly component. It runs in a sandbox with no file system, no
processes, no network and no WASI, and reaches exactly the interfaces its kind is made of and its
manifest grants - nothing is granted implicitly and nothing can be requested at run time. The
handbook is the wiki's page *Plugin development*
(<https://github.com/degoya/rCommander/wiki/plugin-development>), whose last part, *The
contract*, is generated from the WIT by `scripts/wit-reference.sh`.

Each kind of the contract has a scaffold, one per `--type`. Stage 1 of the plugin system brings
the first kind:

| Type | What it does | The scaffold |
| --- | --- | --- |
| `viewer` | Shows a file in the Lister (F3) and Quick View as text, coloured text, Markdown, a picture, a tree or pages | CSV and TSV as a Markdown table, any other file as text |

The packer, file system, connection, content, secret source, command and compare kinds follow
with their stages, each with its scaffold.

A scaffold is a small, working plugin of its kind rather than a set of signatures, with the
promises the host makes to that kind written at the top of `src/lib.rs`. They shape the code, so
they are worth reading before changing it. Everything that can run without a WebAssembly
toolchain lives outside the component, and `src/guest.rs` - the translation into the WIT
vocabulary - exists only on `wasm32`, so `cargo test` runs a fresh scaffold's tests straight
away. Each carries a `CHANGES.md` for its release notes, a `## <version>` section per version in
one to three sentences for the people who install it, and a `README.md`.
`scripts/check-sdk-templates.sh` fails when a world of the contract has no scaffold, a scaffold
lacks its `README.md`, `CHANGES.md` or a test, its `wit/` differs from the contract, or its
`wit-bindgen`, `api_version`, `min_app_version` or the release `ci/plugin.yml` pins disagree.

## Start

```bash
rcommander plugin new --type viewer --out ./my_viewer
cd my_viewer
cargo test
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/my_viewer.wasm \
  -o target/my_viewer.wasm
```

`plugin new` carries the scaffolds inside `rcommander.exe`, so it needs no checkout of
rCommander. The directory name becomes the crate name and the key's name; `--name` sets a
different one. The scaffold needs the `wasm32-unknown-unknown` target (`rustup target add
wasm32-unknown-unknown`) and `wasm-tools` 1.261.0 - the version rCommander's own plugins are made
with, `cargo install wasm-tools --version 1.261.0 --locked`. `cargo build` makes a core module
that carries the world `src/guest.rs` generates; `wasm-tools component new` turns it into the
component, with no WASI adapter, because a plugin imports nothing but the contract.

The scaffold compiles, packages and passes conformance before you change a line of it - so the
first failure you see is about your code and not about the setup. It brings its own copy of the
contract in `wit/`, its own id (a UUIDv7) and its own Ed25519 key pair in `plugin-signing.key`,
which belongs in your secret store and not in version control. `plugin new` writes a
`.gitignore` before the key, listing `plugin-signing.key`, `target/` and `*.rcplug`, so a first
`git add .` leaves the key out; keep those lines when you edit the file.

```bash
rcommander plugin package --manifest manifest.toml \
  --component target/my_viewer.wasm \
  --locales locales --key plugin-signing.key --output my_viewer.rcplug
rcommander plugin conformance my_viewer.rcplug \
  --trusted-key my_viewer-release-v1=<the public_key of manifest.toml>
```

`--key` may be left out when `RCOMMANDER_PLUGIN_SIGNING_KEY` holds the PEM text, which is how the
CI workflow below signs. `--development` writes an unsigned package instead; only development
mode (Settings -> Plugins, `plugins.development_mode`, and `--development-mode` of `verify` and
`conformance`) accepts one, so it is for a local test and never for a release.
`rcommander plugin keygen --out <file>` makes another key pair, for a release key kept apart
from the one `plugin new` wrote.

On Windows `rcommander.exe` is a program with a window: started from a console it writes into
that console, but the prompt may come back before it is done. `start /wait rcommander plugin
...` in cmd or `Start-Process -Wait` in PowerShell wait for it and its exit code.

## Translations

The scaffold ships `locales/en.json`, and English is the one language a localised plugin must
carry: a package with any locale file and no `en.json` is refused. Every code in it starts with
the plugin's id (`<id>.<condition>`): a plugin can neither override rCommander's own texts nor
another plugin's, and a failure code outside that namespace is dropped by the host.

## What conformance checks

`plugin conformance` answers one question: would this rCommander run your package? It verifies
the archive, the manifest, the API version, the locales, the signature, the limits and every
import exactly as installation does (`package`), instantiates the component against the world
its kind declares (`world`), and asks it one question of its kind without a network:

| Check | Kind | The question |
| --- | --- | --- |
| `viewer_empty_source` | viewer | `probe`, `open` and the first output the document names, on an empty file named like one `[match]` claims, end in an answer or a refusal - never a trap, an exhausted budget or an answer against the rules |

It does **not** check that your viewer shows a real file correctly. A package that passes every
check can still be wrong about its format; the report says what was verified, not that the plugin
is good. `--json` prints a report with one entry per check, and the command exits non-zero if
anything failed, so a CI job can gate on it without parsing output.

## Capabilities

Each grant in `[capabilities]` is one WIT interface. A component that imports an interface its
manifest does not grant is refused when it is packaged, again when it is installed, and it cannot
be instantiated at all - so asking for less than you use fails at build time rather than on a
person's file. A viewer is granted nothing: it reads the file it is handed and nothing else.

A component that imports anything outside `rcommander:plugin` - above all a `wasi:` interface
pulled in by a dependency - is refused the same way. `scripts/check-plugin-imports.sh` in
rCommander's repository reads the import section with `wasm-tools` and names the interface; the
CI workflow below runs the same check.

## CI

`ci/plugin.yml` is a GitHub Actions workflow that tests, builds, rejects imports outside the
contract, packages, verifies and runs conformance. Copy it into `.github/workflows/`, set
`PLUGIN_SIGNING_KEY` as a repository secret holding the PEM text of `plugin-signing.key`, and put
your crate name into `COMPONENT`. It runs on a Windows runner with the rCommander release named in
`RCOMMANDER_VERSION`, checked against the release's `SHA256SUMS` before it runs. The pin is the
release this SDK came with, which accepts the scaffolds' `api_version`; bump it when you build
against a newer contract.

The workflow reads the repository and nothing more (`permissions: contents: read`), and its
actions are pinned to commits. Only a push to the default branch or a tag signs: the signing key
reaches that one step and no other, while a pull request builds an unsigned development package
and runs the same verification and conformance in development mode.

## The contract

`wit/rcommander.wit` in your scaffold is the contract. `api_version` in your manifest names the
WIT package version you built against (`rcommander:plugin@0.2.0`), and an rCommander that does
not support it refuses your package rather than running it against a contract you did not write
for. A template's `min_app_version` is the release the SDK came with, the first that accepts its
`wit/` copy. There is no compatibility promise before the contract's 1.0; an rCommander still
runs a package built against the contract before (0.1.0, without `settings`).

A plugin reads its own `[[settings]]` through the `settings` interface (since 0.2.0); it may
import `settings` only when its manifest declares at least one setting. A plugin that never calls
it does not import it.

## Licence

The SDK - the scaffolds and the CI workflow - is MIT licensed, unlike rCommander itself
(GPL-3.0-or-later), so a plugin built from it may carry any licence.
