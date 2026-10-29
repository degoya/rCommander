#!/usr/bin/env bash
#
# Verifies that a built plugin component imports nothing outside the `rcommander:plugin`
# contract - above all no `wasi:` interface (RC-100-12). Copied and adapted from rDownloader's
# scripts/check-plugin-imports.sh at commit 26c8592a6.
#
# Why this is not a `grep` over the binary: a byte scan cannot tell an import from a string, so a
# plugin whose data section happens to hold the text - an error message, a dependency's
# diagnostic - would be refused while importing nothing at all. The import section is a
# structured part of the component; reading it with `wasm-tools component wit` is the only check
# that answers the question asked. Which interfaces of the contract a plugin may import is the
# host's question, asked when it is packaged, installed and instantiated (rc-plugin-host's
# `check_component`); this script guards the step before, the component build.
#
# `wasm-tools` is optional here. Without it the byte scan runs instead, loudly, so a machine that
# lacks the tool still catches the obvious case rather than silently checking nothing.
#
# Usage:
#   scripts/check-plugin-imports.sh <component.wasm> [<component.wasm> ...]
#
set -euo pipefail

# Everything the host offers lives under `rcommander:plugin`; anything else means the component
# reached for something the sandbox does not have.
ALLOWED_PREFIX="rcommander:plugin/"

[[ $# -gt 0 ]] || { echo "usage: scripts/check-plugin-imports.sh <component.wasm>..." >&2; exit 2; }

if ! command -v wasm-tools > /dev/null; then
    echo "!! wasm-tools is not installed - falling back to a byte scan, which cannot tell an" >&2
    echo "   import from a string literal. Install it for the real check (docs/development.md)." >&2
    status=0
    for component in "$@"; do
        if grep -a -q 'wasi:' "$component"; then
            echo "!! $component contains the text 'wasi:' (byte scan; may be a false positive)" >&2
            status=1
        fi
    done
    exit "$status"
fi

status=0
for component in "$@"; do
    [[ -f "$component" ]] || { echo "!! $component does not exist" >&2; status=1; continue; }

    # `component wit` renders the component's world. Its `import` lines name interfaces; a file
    # that is not a component fails here rather than passing unchecked.
    if ! world="$(wasm-tools component wit "$component" 2>&1)"; then
        echo "!! $component is not a readable WebAssembly component:" >&2
        printf '%s\n' "$world" | sed 's/^/   /' >&2
        status=1
        continue
    fi

    # `import foo:bar/baz@1.2.3;` -> `foo:bar/baz@1.2.3`
    imports="$(printf '%s\n' "$world" \
        | sed -n 's/^[[:space:]]*import[[:space:]]\{1,\}\([^;]*\);.*$/\1/p' \
        | sed 's/[[:space:]]*$//')"

    offenders=""
    while IFS= read -r interface; do
        [[ -n "$interface" ]] || continue
        [[ "$interface" == "$ALLOWED_PREFIX"* ]] && continue
        offenders+="   $interface"$'\n'
    done <<< "$imports"

    if [[ -n "$offenders" ]]; then
        echo "!! $component imports outside $ALLOWED_PREFIX:" >&2
        printf '%s' "$offenders" >&2
        status=1
    fi
done

exit "$status"
