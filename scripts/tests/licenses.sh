#!/usr/bin/env bash
#
# scripts/lib/licenses.py against made-up `cargo metadata` and `cargo tree` output in a temp
# directory: the list is written with the linked crates and their licences and the others as not
# shipped, a workspace member and a repeated crate (`(*)`) are no entries, --check passes on it, a
# new linked crate makes --check fail as stale, and a crate without a licence is refused with
# exit 2 until an override names one - as is an override for no locked crate.
#
# Pure python3 and bash, no cargo: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/licenses.sh
# shellcheck disable=SC2016  # the condition is single-quoted on purpose: expect_true evaluates it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/lib/licenses.py"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
METADATA="$SCRATCH/metadata.json"
LINKED="$SCRATCH/linked.txt"
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
LIST="$TREE/crates/rcommander/resources/licenses/third-party.json"
OVERRIDES="$TREE/crates/rcommander/resources/licenses/overrides.json"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$(dirname "$OVERRIDES")"
printf '{}\n' > "$OVERRIDES"
write_metadata() {
    cat > "$METADATA" <<JSON
{"packages": [
  {"name": "rcommander", "version": "1.2.3", "source": null, "license": "GPL-3.0-or-later"},
  {"name": "iced", "version": "0.14.0", "source": "registry+https://github.com/rust-lang/crates.io-index", "license": "MIT"},
  {"name": "serde", "version": "1.0.0", "source": "registry+https://github.com/rust-lang/crates.io-index", "license": "MIT OR Apache-2.0"},
  {"name": "serde_derive", "version": "1.0.0", "source": "registry+https://github.com/rust-lang/crates.io-index", "license": "MIT OR Apache-2.0"},
  {"name": "quiet", "version": "0.3.0", "source": "registry+https://github.com/rust-lang/crates.io-index", "license": $1}
]}
JSON
}
write_metadata '"Zlib"'
printf '%s\n' 'rcommander v1.2.3 (/src/crates/rcommander)' 'iced v0.14.0' 'serde v1.0.0' 'iced v0.14.0 (*)' > "$LINKED"

run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "the list is written" 0
expect_true "a linked crate with its licence" 'grep -qF "{\"name\": \"iced\", \"version\": \"0.14.0\", \"license\": \"MIT\"}" "$LIST"'
expect_true "a proc macro is not shipped" 'grep -qF "\"serde_derive@1.0.0\"" "$LIST"'
expect_true "the workspace member is no entry" '! grep -qF "rcommander" "$LIST"'
expect "two crates are listed" "2" "$(grep -c '"license"' "$LIST")"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED" --check
expect_status "--check passes on a current list" 0

echo 'quiet v0.3.0' >> "$LINKED"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED" --check
expect_status "--check fails on a stale list" 1
expect_output "--check names the script" "run scripts/licenses.sh"

write_metadata 'null'
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "a crate without a licence is refused" 2
expect_output "the crate is named" "quiet@0.3.0: declares no licence"

printf '{"quiet@0.3.0": {"license": "BSD-3-Clause", "source": "LICENSE in the crate"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "an override names the licence" 0
expect_true "the override is listed" 'grep -qF "{\"name\": \"quiet\", \"version\": \"0.3.0\", \"license\": \"BSD-3-Clause\"}" "$LIST"'

printf '{"gone@9.9.9": {"license": "MIT", "source": "long ago"}}\n' > "$OVERRIDES"
write_metadata '"Zlib"'
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "an override for no locked crate is refused" 2
expect_output "the stale override is named" "gone@9.9.9: an override for no locked crate"

finish_tests licenses
