# {{PLUGIN_NAME}}

A `viewer` plugin for rCommander, scaffolded by `rcommander plugin new --type viewer`. It shows
CSV and TSV files as a table - a Markdown table the viewer renders in F3 and Quick View - and any
other file it is handed as text.

Read the rCommander wiki's page *Plugin development* (`develop/plugin-development.md`) before
changing it: it describes what the host promises a viewer, what it refuses, and the contract
function by function.

## What is here

- `src/lib.rs` - `Viewer` (`open`, `text`) and `probe`, with the host's promises at the top
- `src/table.rs` - CSV and TSV into rows, rows into a Markdown table
- `src/types.rs` - the viewer world's shapes the logic uses, mirrored from the contract
- `src/guest.rs` - the translation into the contract's vocabulary, only for `wasm32`
- `src/lib_tests.rs` - the tests, run with `cargo test`
- `manifest.toml` - identity, `[match]`, `[viewer] outputs`, limits, the `separator` setting;
  `plugin_type = "viewer"`
- `locales/en.json` - the name and description in English, and the translation of every
  failure code the plugin reports (`<plugin id>.<condition>`)
- `CHANGES.md` - the release notes, a `## <version>` section per version in one to three
  sentences for the people who install it
- `wit/rcommander.wit` - the contract, a copy of the one rCommander speaks

It asks for nothing: a viewer reads the file it is handed and nothing else. Its one setting,
`separator`, is the person's choice on the plugin's page in Settings -> Plugins; `open` reads it
through the contract's `settings` interface, which only a plugin declaring `[[settings]]` may
import. The install preview
shows the person every grant, so ask for exactly what you use.

## Three rules worth knowing first

- A document names its outputs in `open`, the first shown, each one listed in `[viewer]
  outputs`; the host refuses any other.
- Read in windows of at most 1 MiB and ask `should-stop` between them: Quick View moves on to
  the next file, and a viewer that keeps reading is ended.
- A failure code is yours only under your plugin's id (`<id>.<condition>`) and translated in
  `locales/en.json`; the host drops any other.

## Build, test, package

```bash
cargo test
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/{{PLUGIN_SLUG}}.wasm \
  -o target/{{PLUGIN_SLUG}}.wasm
rcommander plugin package --manifest manifest.toml \
  --component target/{{PLUGIN_SLUG}}.wasm \
  --locales locales --key plugin-signing.key --output {{PLUGIN_SLUG}}.rcplug
rcommander plugin conformance {{PLUGIN_SLUG}}.rcplug \
  --trusted-key {{PLUGIN_SLUG}}-release-v1=<the public_key of manifest.toml>
```

`cargo test` runs the tests on your own machine, without a WebAssembly toolchain. `cargo build`
makes a WebAssembly core module that carries the world `src/guest.rs` generates, and
`wasm-tools component new` (1.261.0, `cargo install wasm-tools --version 1.261.0 --locked`)
turns it into the component - no WASI adapter, the plugin imports nothing but the contract.
`plugin-signing.key` is yours alone: keep it out of version control. Install the package in
rCommander from Settings -> Plugins -> "Install...", which shows its key's fingerprint for you to
confirm.

Licence: MIT.
