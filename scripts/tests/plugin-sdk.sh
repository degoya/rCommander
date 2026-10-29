#!/usr/bin/env bash
#
# The plugin SDK's scripts (RC-100-12) against copies in a temp directory:
#
# - check-sdk-templates.sh passes on the tree's SDK and names each gap: a template's contract
#   copy that differs, a `min_app_version` or a CI pin that is not the workspace version, a
#   template folder no world names, a template file the executable does not compile in;
# - wit-reference.sh reads the tree's contract completely, refuses a construct it does not know
#   and a job id outside a parenthesis, writes the wiki page between its markers, and --check
#   tells a current page from a stale one;
# - check-plugin-imports.sh passes a component importing the contract only and refuses one
#   importing `wasi:` (with wasm-tools; skipped without it);
# - plugin-manifest-defaults.sh --check finds a stated default, and the conversion removes it.
#
# Pure bash and python3 (and wasm-tools where installed): it runs in a second. check.sh
# --preflight runs it.
#
#   scripts/tests/plugin-sdk.sh
# shellcheck disable=SC2016  # the condition is single-quoted on purpose: expect_true evaluates it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

# --- check-sdk-templates.sh ---------------------------------------------------------------

mkdir -p "$TREE/crates/rc-plugin-api/src" "$TREE/crates/rcommander/src/plugin_cli" "$TREE/scripts/lib"
cp "$ROOT/Cargo.toml" "$TREE/"
cp -r "$ROOT/crates/rc-plugin-api/wit" "$TREE/crates/rc-plugin-api/"
cp "$ROOT/crates/rc-plugin-api/src/lib.rs" "$TREE/crates/rc-plugin-api/src/"
cp "$ROOT/crates/rcommander/src/plugin_cli/scaffold.rs" "$TREE/crates/rcommander/src/plugin_cli/"
cp -r "$ROOT/sdk" "$TREE/"
TEMPLATE="$TREE/sdk/templates/viewer"

run_status bash "$ROOT/scripts/check-sdk-templates.sh" "$TREE"
expect_status "the tree's SDK is complete" 0
expect_output "it counts the worlds" "2 world(s), 2 template(s)"

echo "// changed" >> "$TEMPLATE/wit/rcommander.wit"
sed -i 's/^min_app_version = ".*"$/min_app_version = "0.0.1"/' "$TEMPLATE/manifest.toml"
sed -i 's/^  RCOMMANDER_VERSION: .*$/  RCOMMANDER_VERSION: 9.9.9/' "$TREE/sdk/ci/plugin.yml"
mkdir -p "$TREE/sdk/templates/packer"
echo "notes" > "$TEMPLATE/NOTES.md"
run_status bash "$ROOT/scripts/check-sdk-templates.sh" "$TREE"
expect_status "a template out of step fails" 1
expect_output "a changed contract copy" "wit/rcommander.wit is not the current contract"
expect_output "an old min_app_version" "says min_app_version 0.0.1, the workspace is"
expect_output "a CI pin" "sdk/ci/plugin.yml pins rCommander 9.9.9"
expect_output "a folder no world names" "sdk/templates/packer belongs to no world"
expect_output "a file not compiled in" "sdk/templates/viewer/NOTES.md is not compiled in"
expect_output "every finding counted" "5 finding(s)"

# --- wit-reference.sh ---------------------------------------------------------------------

run_status bash "$ROOT/scripts/wit-reference.sh" --check
expect_status "the tree's contract reads completely" 0
run_status bash "$ROOT/scripts/wit-reference.sh" --print
expect_output "a world with its imports" '| `viewer-plugin` | `host`, `source`, `sink`, `settings` | `viewer` | `--type viewer` |'
expect_output "a comment after the code" '`read-at: func(src: source, offset: u64, length: u32) -> result<list<u8>, failure>` - &lt;= 1 MiB'
expect_output "an escaped case name" '`keyword`, `key`, `string`, `number`'

WIT_TREE="$SCRATCH/wit"
WIT_FILE="$WIT_TREE/crates/rc-plugin-api/wit/rcommander.wit"
mkdir -p "$(dirname "$WIT_FILE")" "$WIT_TREE/scripts/lib" "$SCRATCH/wiki/develop"
cp "$ROOT/scripts/lib/wit-reference.py" "$WIT_TREE/scripts/lib/"
cat > "$WIT_FILE" << 'EOF'
// The preamble names jobs (RC-100-08) and is not part of the reference.
package rcommander:plugin@0.1.0;

// Comes first (RC-100-08).
interface host {
  log: func(level: string, message: string);   // capped and masked
}

world viewer-plugin {
  import host;
  export host;
}
EOF
PAGE="$SCRATCH/wiki/develop/plugin-development.md"
printf '# Plugins\n\n<!-- BEGIN wit-reference -->\nold\n<!-- END wit-reference -->\n\nAfter.\n' > "$PAGE"
run_status python3 -B "$WIT_TREE/scripts/lib/wit-reference.py" "$WIT_TREE" --wiki "$SCRATCH/wiki"
expect_status "the page is written" 0
expect_true "a comment above the statement, its job id left out" 'grep -qx "Comes first." "$PAGE"'
expect_true "the preamble stays out" '! grep -qF "preamble" "$PAGE"'
expect_true "the text around the markers stays" 'grep -qx "After." "$PAGE" && ! grep -qx "old" "$PAGE"'
run_status python3 -B "$WIT_TREE/scripts/lib/wit-reference.py" "$WIT_TREE" --wiki "$SCRATCH/wiki" --check
expect_status "--check passes on a current page" 0
sed -i 's/capped and masked/capped/' "$WIT_FILE"
run_status python3 -B "$WIT_TREE/scripts/lib/wit-reference.py" "$WIT_TREE" --wiki "$SCRATCH/wiki" --check
expect_status "--check fails on a stale page" 1
sed -i 's/capped$/capped, see RC-100-08/' "$WIT_FILE"
run_status python3 -B "$WIT_TREE/scripts/lib/wit-reference.py" "$WIT_TREE" --check
expect_status "a job id outside a parenthesis is refused" 2
sed -i 's/, see RC-100-08//' "$WIT_FILE"
printf 'interface later {\n  resource handle {\n  }\n}\n' >> "$WIT_FILE"
run_status python3 -B "$WIT_TREE/scripts/lib/wit-reference.py" "$WIT_TREE" --check
expect_status "an unknown construct is refused" 2
expect_output "and named" "resource handle"
printf '# Plugins\n' > "$PAGE"
sed -i '/^interface later/,$d' "$WIT_FILE"
run_status python3 -B "$WIT_TREE/scripts/lib/wit-reference.py" "$WIT_TREE" --wiki "$SCRATCH/wiki"
expect_status "a page without markers is refused" 2

# --- check-plugin-imports.sh --------------------------------------------------------------

if command -v wasm-tools > /dev/null; then
    printf '(component (import "rcommander:plugin/host@0.1.0" (instance)))\n' > "$SCRATCH/good.wat"
    printf '(component (import "wasi:cli/environment@0.2.0" (instance)) (import "rcommander:plugin/host@0.1.0" (instance)))\n' \
        > "$SCRATCH/bad.wat"
    wasm-tools parse "$SCRATCH/good.wat" -o "$SCRATCH/good.wasm"
    wasm-tools parse "$SCRATCH/bad.wat" -o "$SCRATCH/bad.wasm"
    printf 'not a component' > "$SCRATCH/junk.wasm"
    run_status bash "$ROOT/scripts/check-plugin-imports.sh" "$SCRATCH/good.wasm"
    expect_status "a component importing the contract only passes" 0
    run_status bash "$ROOT/scripts/check-plugin-imports.sh" "$SCRATCH/bad.wasm"
    expect_status "an import outside the contract is refused" 1
    expect_output "and named" "wasi:cli/environment@0.2.0"
    run_status bash "$ROOT/scripts/check-plugin-imports.sh" "$SCRATCH/junk.wasm"
    expect_status "a file that is no component is refused" 1
else
    ok "check-plugin-imports.sh: skipped, wasm-tools is not installed"
fi

# --- plugin-manifest-defaults.sh ----------------------------------------------------------

DEFAULTS_TREE="$SCRATCH/defaults"
mkdir -p "$DEFAULTS_TREE/plugins/demo" "$DEFAULTS_TREE/crates/rc-plugin-host/src" "$DEFAULTS_TREE/scripts/lib"
cp "$ROOT/scripts/lib/plugin-manifest-defaults.py" "$DEFAULTS_TREE/scripts/lib/"
cp "$ROOT/plugins/metadata.toml" "$DEFAULTS_TREE/plugins/"
cp "$ROOT/crates/rc-plugin-host/src/limits.rs" "$DEFAULTS_TREE/crates/rc-plugin-host/src/"
MANIFEST="$DEFAULTS_TREE/plugins/demo/manifest.toml"
cat > "$MANIFEST" << 'EOF'
plugin_type = "viewer"
api_version = "0.1.0"
id = "019d0000-0000-7000-8000-0000000000a1"

[metadata]
description = "A demo."
license = "MIT"

[limits]
memory_bytes = 67108864
timeout_milliseconds = 1000
EOF
run_status python3 -B "$DEFAULTS_TREE/scripts/lib/plugin-manifest-defaults.py" "$DEFAULTS_TREE" --check
expect_status "a stated default is found" 1
expect_output "and named" "states a default: plugins/demo/manifest.toml"
run_status python3 -B "$DEFAULTS_TREE/scripts/lib/plugin-manifest-defaults.py" "$DEFAULTS_TREE"
expect_status "the conversion runs" 0
expect_true "the shared and default lines go" '! grep -qE "^(api_version|license|memory_bytes)" "$MANIFEST"'
expect_true "the rest stays" 'grep -qx "timeout_milliseconds = 1000" "$MANIFEST" && grep -qx "description = \"A demo.\"" "$MANIFEST"'
run_status python3 -B "$DEFAULTS_TREE/scripts/lib/plugin-manifest-defaults.py" "$DEFAULTS_TREE" --check
expect_status "a converted manifest passes" 0
run_status bash "$ROOT/scripts/plugin-manifest-defaults.sh" --check
expect_status "the tree's bundled manifests state no default" 0

finish_tests plugin-sdk
