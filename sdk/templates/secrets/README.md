# {{PLUGIN_NAME}}

A secret source for rCommander: a password manager as the source of a connection's password,
private key or passphrase. rCommander asks it while it connects and keeps the answer in memory
only; the plugin never receives a secret of rCommander's.

## What is here

- `manifest.toml` - who the plugin is and what it may reach: `net_http = { loopback = 8087 }`
- `src/lib.rs` - the logic, plain Rust: `status`, `search` and `get` against a local bridge
- `src/types.rs` - the contract's shapes the logic uses
- `src/guest.rs` - the translation into the contract, built for `wasm32` only
- `src/lib_tests.rs` - tests against a bridge that answers from a table
- `locales/en.json` - the plugin's name, description and failure codes
- `wit/rcommander.wit` - the contract, a copy of the one rCommander speaks

The example speaks to a bridge on `localhost:8087` with three plain-text requests (`/status`,
`/search?q=`, `/secret/<id>/<field>`). Replace them with your manager's own local API - a bridge
such as `bw serve` - and change the port in `manifest.toml` to the one it listens on. A secret
source reaches that one port of this machine over plain HTTP and nothing else: the install
preview shows it, and installing is the person's confirmation.

## Three rules worth knowing first

- `search` answers ids, titles, users and addresses - never a value. Drop whatever else your
  manager's search answers before you hand entries back.
- Unlocking is the manager's own business. Answer `locked` from `status` and let the person
  unlock in the manager; never ask for a master password.
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

`cargo test` runs the tests on your own machine, without a WebAssembly toolchain. The
conformance run asks the source while its port is refused, as when the manager is not running:
`status` and `search` have to answer or refuse, not crash. `plugin-signing.key` is yours alone:
keep it out of version control.

Licence: MIT.
