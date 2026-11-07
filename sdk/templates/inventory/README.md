# {{PLUGIN_NAME}}

An inventory for rCommander: the hosts of a server inventory (NetBox, Proxmox, your own CMDB) as
connections. rCommander keeps one folder of connections in step with the plugin's list; the
person's own fields of each connection - credentials, login script, the drive-bar mark - stay as
they are, and a host that leaves the inventory is marked, never deleted.

## What is here

- `manifest.toml` - who the plugin is and what it may reach: `net_http = { entered = true }`
  and the token slot `secrets = ["api_token"]`
- `src/lib.rs` - the logic, plain Rust: `list` against one plain-text request
- `src/types.rs` - the contract's shapes the logic uses
- `src/guest.rs` - the translation into the contract, built for `wasm32` only
- `src/lib_tests.rs` - tests against a server that answers from a string
- `locales/en.json` - the plugin's name, description and failure codes
- `wit/rcommander.wit` - the contract, a copy of the one rCommander speaks

The example asks `GET /hosts.tsv` and reads one host per line. Replace it with your inventory's
own API. The path is relative: rCommander resolves it against the address the person entered
for the source, and refuses any request to another scheme, host or port.

## Three rules worth knowing first

- The token is never yours. Write `{{secret:api_token}}` into a header value and rCommander puts
  the token the person stored there, on the way out, to the entered address only. A marker in
  the address, in the body or in `Referer`/`Origin` is refused; what a server echoes back is
  masked in your log lines and failures.
- The `external_id` is what a connection is matched by. Use the inventory's own stable id, not
  the name: a renamed host stays the same connection.
- rCommander checks every host again: at most 10 000, an address that is a host name or an IP
  address and nothing else, a port other than 0, no control character, no `.` or `..` in the
  folder path. A host breaking a rule is left out and counted, not repaired.

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
conformance run asks the inventory without an address, as when its server is not reachable:
`list` has to answer or refuse, not crash. `plugin-signing.key` is yours alone: keep it out of
version control.

Licence: MIT.
