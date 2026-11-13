#!/usr/bin/env bash
#
# scripts/lib/tool-versions.sh is the one source of the tools' versions (RC-110-33). An action
# that takes a version as a literal (`tool: nextest@0.9.146`) cannot source it, so this test
# holds the literals to it: every `tool: <name>@<version>` of the workflows and the composite
# actions names a pinned tool at its pin, taiki-e/install-action is one commit everywhere, komac
# and nfpm are named by no workflow (scripts/release-channels.sh reads their pins), and the
# table of docs/development.md - where the tree has it; the public export leaves docs/ out -
# names the same versions.
#
# Pure bash: it runs at once. check.sh --preflight runs it.
#
#   scripts/tests/tool-versions.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=../lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
cd "$ROOT"

# The pin of a tool as taiki-e/install-action names it; nothing for a tool without one.
pin_of() {
    case "$1" in
        nextest) echo "$RC_NEXTEST_VERSION" ;;
        wasm-tools) echo "$RC_WASM_TOOLS_VERSION" ;;
        cargo-xwin) echo "$RC_CARGO_XWIN_VERSION" ;;
        cargo-fuzz) echo "$RC_CARGO_FUZZ_VERSION" ;;
        cargo-llvm-cov) echo "$RC_LLVM_COV_VERSION" ;;
    esac
}

# Every `tool:` line below .github, as `<file>:<tool>[@<version>]`.
tools="$(grep -rnE '^[[:space:]]*tool:' .github --include='*.yml' | sed -E 's/^([^:]+):[0-9]+:[[:space:]]*tool:[[:space:]]*/\1:/')"
expect_true "the workflows install tools through \`tool:\` lines" '[[ -n "$tools" ]]'
while IFS= read -r line; do
    file="${line%%:*}"
    for entry in ${line#*:}; do
        entry="${entry%,}"
        name="${entry%%@*}"
        expect "$file: $name is at its pin in scripts/lib/tool-versions.sh" "$name@$(pin_of "$name")" "$entry"
    done
done <<< "$tools"

actions="$(grep -rhoE 'taiki-e/install-action@[0-9a-f]+' .github --include='*.yml' | sort -u)"
expect "taiki-e/install-action is one commit in every workflow and action" 1 "$(grep -c . <<< "$actions")"

expect "no workflow names komac's or nfpm's version: scripts/release-channels.sh reads the pins" "" \
    "$(grep -rnE 'KOMAC_(VERSION|SHA256)|NFPM_(VERSION|SHA256)|nfpm_[0-9]' .github --include='*.yml' || true)"
expect_true "release-channels.sh downloads komac and nfpm at the pins" \
    'grep -qF "v\${RC_KOMAC_VERSION}/" scripts/release-channels.sh && grep -qF "\${RC_KOMAC_SHA256}" scripts/release-channels.sh && grep -qF "v\${RC_NFPM_VERSION}/" scripts/release-channels.sh && grep -qF "\${RC_NFPM_SHA256}" scripts/release-channels.sh'

# The documentation's table: `| <tool> | <version> | <how to install> |`.
DOC="docs/development.md"
if [[ -f "$DOC" ]]; then
    row() { grep -E "^\| $1 \|" "$DOC" | head -n 1; }
    documented() {
        local tool="$1" version="$2"
        expect_true "$DOC names $tool $version" '[[ "$(row "$tool")" == "| $tool | $version"* ]]'
    }
    documented cargo-xwin "$RC_CARGO_XWIN_VERSION"
    documented cargo-nextest "$RC_NEXTEST_VERSION"
    documented shellcheck "$RC_SHELLCHECK_VERSION"
    documented wasm-tools "$RC_WASM_TOOLS_VERSION"
    documented "Rust nightly" "$RC_FUZZ_NIGHTLY"
    documented cargo-fuzz "$RC_CARGO_FUZZ_VERSION"
    documented cargo-llvm-cov "$RC_LLVM_COV_VERSION"
    expect_true "and the install commands name the same versions" \
        'grep -qF "cargo-xwin --version $RC_CARGO_XWIN_VERSION " "$DOC" && grep -qF "cargo-nextest --version $RC_NEXTEST_VERSION " "$DOC" && grep -qF "wasm-tools --version $RC_WASM_TOOLS_VERSION " "$DOC" && grep -qF "cargo-fuzz --version $RC_CARGO_FUZZ_VERSION " "$DOC" && grep -qF "cargo-llvm-cov --version $RC_LLVM_COV_VERSION " "$DOC" && grep -qF "rustup toolchain install $RC_FUZZ_NIGHTLY " "$DOC"'
    expect_true "gitleaks and nfpm at their CI pins" \
        'grep -qF "the pinned $RC_GITLEAKS_VERSION" "$DOC" && grep -qF "release.yml installs $RC_NFPM_VERSION " "$DOC"'
else
    echo "skip $DOC is not in this tree (the public export leaves docs/ out)"
fi

finish_tests tool-versions
