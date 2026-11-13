#!/usr/bin/env bash
#
# The workspace has no doctests, and stays without (RC-110-33): `cargo nextest run` runs none, and
# a `cargo test --doc` over twenty crates would be a build of its own for examples nobody wrote.
# Every code block in a doc comment is therefore marked as something else than Rust - `text`,
# `json`, `toml`, `sh` - so rustdoc would not compile it either. This test reads the doc comments
# of crates/, plugins/ and sdk/ and names a block rustdoc would take for a doctest: a fence
# without a language or with a Rust one (`rust`, `no_run`, `should_panic`, `compile_fail`,
# `ignore`, an edition), and a block indented by four spaces after an empty doc line. Such an
# example becomes a unit test in the sibling `_tests.rs`, or its fence gets `text`.
#
# Pure bash and awk: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/doc-fences.sh [<directory>...]    # default: crates plugins sdk
# shellcheck disable=SC2016  # the awk program is single-quoted on purpose
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

# Prints `<file>:<line>: <what>` for every block of the Rust files below the directories given
# that rustdoc would compile.
doctests() {
    find "$@" -path '*/target' -prune -o -type f -name '*.rs' -print0 2> /dev/null \
        | LC_ALL=C sort -z | xargs -0 -r awk '
        FNR == 1 { fenced = 0; blank = 0 }
        !/^[ \t]*\/\/[\/!]/ { fenced = 0; blank = 0; next }
        {
            text = $0
            sub(/^[ \t]*\/\/[\/!]/, "", text)
            if (match(text, /^ ? ? ?```/)) {
                info = substr(text, RLENGTH + 1)
                # A backtick after the three is inline code, not a fence (CommonMark).
                if (index(info, "`") == 0) {
                    if (fenced) { fenced = 0; next }
                    fenced = 1
                    gsub(/[ \t{}.]/, "", info)
                    if (info == "" || info ~ /^(rust|no_run|should_panic|compile_fail|ignore|edition)/)
                        printf "%s:%d: a fence rustdoc compiles (```%s); mark it `text` or make it a test\n", FILENAME, FNR, info
                    next
                }
            }
            if (fenced) next
            if (text ~ /^[ \t]*$/) { blank = 1; next }
            if (blank && text ~ /^     /)
                printf "%s:%d: an indented block rustdoc compiles; fence it as `text`\n", FILENAME, FNR
            blank = 0
        }'
}

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
mkdir -p "$SCRATCH/src"
cat > "$SCRATCH/src/lib.rs" << 'RUST'
//! A crate.
//!
//! ```text
//! not Rust
//!     indented inside a fence
//! ```
//! ``` ``literal`` ``` is inline code.

/// An example:
/// ```
/// let x = 1;
/// ```
/// And one more:
/// ```rust,no_run
/// loop {}
/// ```
///
///     let indented = 1;
///
/// - a list
///   continued
pub fn documented() {}
// ``` in a plain comment is nobody's fence
RUST
found="$(doctests "$SCRATCH")"
expect "a fence without a language, a Rust one and an indented block are named, nothing else" \
    "$SCRATCH/src/lib.rs:10: a fence rustdoc compiles (\`\`\`); mark it \`text\` or make it a test
$SCRATCH/src/lib.rs:14: a fence rustdoc compiles (\`\`\`rust,no_run); mark it \`text\` or make it a test
$SCRATCH/src/lib.rs:18: an indented block rustdoc compiles; fence it as \`text\`" "$found"

cd "$ROOT"
if [[ $# -gt 0 ]]; then directories=("$@"); else directories=(crates plugins sdk); fi
expect "no doc comment of ${directories[*]} holds a doctest" "" "$(doctests "${directories[@]}")"

finish_tests doc-fences
