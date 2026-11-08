# {{PLUGIN_NAME}}

A compare plugin for rCommander: two files compared by what they hold rather than line by line.
When a person compares two files by content and both have an extension the manifest claims,
rCommander hands them to the plugin and offers its answer as the Structure view of the compare
window, beside the line diff: every change with its path, its kind (added, removed, changed,
moved), the old and the new value, and the lines a click jumps to.

## What is here

- `manifest.toml` - who the plugin is and which files it compares (`[match] extensions`)
- `src/lib.rs` - the logic, plain Rust: two `key=value` files compared by key
- `src/types.rs` - the contract's shapes the logic uses
- `src/guest.rs` - the translation into the contract, built for `wasm32` only: it reads both
  files through `source` and answers `status` and `compare`
- `src/lib_tests.rs` - tests on two strings
- `locales/en.json` - the plugin's name, description and the text of its notice codes
- `wit/rcommander.wit` - the contract, a copy of the one rCommander speaks

The example compares Java properties files: a reordered line is no change, a key found twice is a
notice. Replace it with your format's own reading.

## Three rules worth knowing first

- The two files are the only thing the plugin reaches: no network, no other file, nothing written.
  Key and credential files are never handed to a plugin, and neither is a file larger than the
  person's limit (`plugins.compare_limit_mib`).
- rCommander checks the answer: at most 10 000 changes - list the first ones and set `truncated`
  when there are more; a longer list is refused as a whole -, every value cut at 1024 characters,
  the counts of the summary never below the changes listed, a notice code only from the plugin's
  own namespace (its id).
- `status` says whether the plugin compares now. A paid plugin without a valid licence answers
  `unlicensed`, and rCommander shows the line diff alone.

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

`cargo test` runs the tests on your own machine, without a WebAssembly toolchain. The
conformance run compares two identical empty files: `compare` has to find no change, or refuse,
not crash. `plugin-signing.key` is yours alone: keep it out of version control.

Licence: MIT.
