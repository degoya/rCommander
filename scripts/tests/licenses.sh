#!/usr/bin/env bash
#
# scripts/lib/licenses.py against made-up `cargo metadata` and `cargo tree` output in a temp
# directory: the list is written with the linked crates and their licences and the others as not
# shipped, a workspace member and a repeated crate (`(*)`) are no entries, --check passes on it, a
# new linked crate makes --check fail as stale, and a crate without a licence is refused with
# exit 2 until an override names one - as is an override for no locked crate.
# THIRD-PARTY-LICENSES (RC-110-72): each text once, headed by every crate it covers; a crate
# without a licence file is refused until its override names a crate to take the texts from or a
# standard text (MIT's with the authors' copyright line); --check fails on a stale text file; and
# scripts/lib/license_dep5.py turns the file into a DEP-5 copyright file.
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
CRATES="$SCRATCH/registry"
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
LIST="$TREE/crates/rcommander/resources/licenses/third-party.json"
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
TEXTS="$TREE/crates/rcommander/resources/licenses/THIRD-PARTY-LICENSES"
OVERRIDES="$TREE/crates/rcommander/resources/licenses/overrides.json"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$(dirname "$OVERRIDES")/texts" "$CRATES"/{iced,iced_core,serde,serde_derive,quiet}
printf '{}\n' > "$OVERRIDES"
printf 'Permission is hereby granted, free of charge.\n' > "$(dirname "$OVERRIDES")/texts/MIT.txt"
printf 'Copyright (c) Iced contributors\r\n\r\nPermission is hereby granted, free of charge, to any person.  \r\n' > "$CRATES/iced/LICENSE"
printf 'Apache License\nVersion 2.0\n' > "$CRATES/serde/LICENSE-APACHE"
printf 'Permission is hereby granted.\n' > "$CRATES/serde/LICENSE-MIT"
printf 'Apache License\nVersion 2.0\n' > "$CRATES/quiet/LICENSE"
write_metadata() {
    local registry='"source": "registry+https://github.com/rust-lang/crates.io-index"'
    cat > "$METADATA" <<JSON
{"packages": [
  {"name": "rcommander", "version": "1.2.3", "source": null, "license": "GPL-3.0-or-later", "manifest_path": "/src/Cargo.toml"},
  {"name": "iced", "version": "0.14.0", $registry, "license": "MIT", "manifest_path": "$CRATES/iced/Cargo.toml"},
  {"name": "iced_core", "version": "0.14.0", $registry, "license": "MIT", "manifest_path": "$CRATES/iced_core/Cargo.toml", "authors": ["Ferris <ferris@example.org>"]},
  {"name": "serde", "version": "1.0.0", $registry, "license": "MIT OR Apache-2.0", "manifest_path": "$CRATES/serde/Cargo.toml"},
  {"name": "serde_derive", "version": "1.0.0", $registry, "license": "MIT OR Apache-2.0", "manifest_path": "$CRATES/serde_derive/Cargo.toml"},
  {"name": "quiet", "version": "0.3.0", $registry, "license": $1, "manifest_path": "$CRATES/quiet/Cargo.toml"}
]}
JSON
}
write_metadata '"Apache-2.0"'
printf '%s\n' 'rcommander v1.2.3 (/src/crates/rcommander)' 'iced v0.14.0' 'serde v1.0.0' 'iced v0.14.0 (*)' > "$LINKED"

run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "the list is written" 0
expect_true "a linked crate with its licence" 'grep -qF "{\"name\": \"iced\", \"version\": \"0.14.0\", \"license\": \"MIT\"}" "$LIST"'
expect_true "a proc macro is not shipped" 'grep -qF "\"serde_derive@1.0.0\"" "$LIST"'
expect_true "the workspace member is no entry" '! grep -qF "rcommander" "$LIST"'
expect "two crates are listed" "2" "$(grep -c '"license"' "$LIST")"
expect_true "the texts are written" '[[ -f "$TEXTS" ]]'
expect_true "a crate's text is headed by the crate and its file" 'grep -qx "iced 0.14.0 (MIT): LICENSE" "$TEXTS"'
expect_true "line ends and trailing blanks are dropped" 'grep -qx "Permission is hereby granted, free of charge, to any person." "$TEXTS" && ! grep -q $'"'"'\r'"'"' "$TEXTS"'
expect "both texts of a dual-licensed crate" "2" "$(grep -c '^serde 1.0.0 ' "$TEXTS")"
expect "a proc macro's text ships nowhere" "0" "$(grep -c '^serde_derive' "$TEXTS")"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED" --check
expect_status "--check passes on current files" 0

echo 'quiet v0.3.0' >> "$LINKED"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED" --check
expect_status "--check fails on a stale list" 1
expect_output "--check names the script" "run scripts/licenses.sh"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "the new crate is written" 0
expect "one Apache text, headed by serde and quiet" "1" \
    "$(python3 -I -c 'import sys; t = open(sys.argv[1]).read().split("=== Licence "); print(sum("Apache License" in b and "quiet 0.3.0" in b and "serde 1.0.0" in b for b in t))' "$TEXTS")"

printf '%s\n' 'the texts by hand' >> "$TEXTS"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED" --check
expect_status "--check fails on a stale text file" 1
expect_output "and names it" "THIRD-PARTY-LICENSES is stale"

write_metadata 'null'
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "a crate without a licence is refused" 2
expect_output "the crate is named" "quiet@0.3.0: declares no licence"

printf '{"quiet@0.3.0": {"license": "BSD-3-Clause", "source": "LICENSE in the crate"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "an override names the licence" 0
expect_true "the override is listed" 'grep -qF "{\"name\": \"quiet\", \"version\": \"0.3.0\", \"license\": \"BSD-3-Clause\"}" "$LIST"'

printf '{"gone@9.9.9": {"license": "MIT", "source": "long ago"}}\n' > "$OVERRIDES"
write_metadata '"Apache-2.0"'
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "an override for no locked crate is refused" 2
expect_output "the stale override is named" "gone@9.9.9: an override for no locked crate"

printf '{}\n' > "$OVERRIDES"
echo 'iced_core v0.14.0' >> "$LINKED"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "a crate without a licence file is refused" 2
expect_output "and named" "iced_core@0.14.0: its archive carries no licence file"

printf '{"iced_core@0.14.0": {"texts_from": "iced@0.14.0", "source": "same repository"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "texts_from gives it the texts of its sibling" 0
expect_true "under the sibling's block, saying so" \
    'grep -qx "iced_core 0.14.0 (MIT): LICENSE of iced 0.14.0, the crate'"'"'s archive carries none" "$TEXTS"'

printf '{"iced_core@0.14.0": {"standard": "MIT", "source": "the standard text"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "standard gives it the standard text" 0
expect_true "MIT's with the authors' copyright line, no address" 'grep -qx "Copyright (c) Ferris" "$TEXTS"'

printf '{"iced_core@0.14.0": {"standard": "Apache-2.0", "source": "wrong"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "a standard text of another licence is refused" 2
expect_output "and named" "standard Apache-2.0 is not a licence of 'MIT'"

printf '{"iced@0.14.0": {"standard": "MIT", "source": "needless"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
expect_status "a texts override for a crate with files is refused" 2
expect_output "and named" "iced@0.14.0: its archive carries licence files"

printf '{"iced_core@0.14.0": {"texts_from": "iced@0.14.0", "source": "same repository"}}\n' > "$OVERRIDES"
run_status python3 -B "$SCRIPT" "$TREE" "$METADATA" "$LINKED"
run_status python3 -B "$ROOT/scripts/lib/license_dep5.py" "$TEXTS" "$SCRATCH/copyright"
expect_status "the DEP-5 copyright is written" 0
expect "it names its format" "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/" \
    "$(head -n 1 "$SCRATCH/copyright")"
expect_true "rCommander itself under the GPL" 'grep -qx "License: GPL-3.0-or-later" "$SCRATCH/copyright"'
expect_true "a crate as its registry source" 'grep -qE "^(Files:)? +iced_core-0.14.0/\*$|^Files: iced_core-0.14.0/\*$" "$SCRATCH/copyright"'
expect_true "the copyright line of its text" 'grep -qx "Copyright: Copyright (c) Iced contributors" "$SCRATCH/copyright"'
expect_true "MIT by its words, the text indented" 'grep -qx "License: MIT" "$SCRATCH/copyright" && grep -qx " Permission is hereby granted, free of charge, to any person." "$SCRATCH/copyright"'
run_status python3 -B "$ROOT/scripts/lib/license_dep5.py" "$SCRATCH/none" "$SCRATCH/copyright"
expect_status "a missing input is refused" 2

finish_tests licenses
