#!/usr/bin/env bash
#
# scripts/icon-font.sh --check and scripts/lib/icon_symbols.py --check (RC-110-10): the symbol
# collection's list, its table and its font as committed are in step; a copy of the table with a
# row changed or missing, or a font over its budget, makes --check fail; a list with an unknown
# category, a name twice or a keyword in upper case does not read (exit 2).
#
# Pure python3 and bash, no network and no fontTools: it runs in a second. check.sh --preflight
# runs it.
#
#   scripts/tests/icon-font.sh
# shellcheck disable=SC2016  # the condition is single-quoted on purpose: expect_true evaluates it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/lib/icon_symbols.py"
LIST="$ROOT/scripts/lib/icon_symbols.txt"
TABLE="$ROOT/crates/rc-engine/src/user_commands/symbols.tsv"
FONT="$ROOT/crates/rcommander/resources/icons/rcommander-symbols.ttf"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

run_status "$ROOT/scripts/icon-font.sh" --check
expect_status "the committed list, table and font are in step" 0

# shellcheck disable=SC2034  # read by the condition expect_true evaluates
symbols="$(grep -cv '^#' "$TABLE")"
expect_true "the collection holds 300 to 500 symbols" '(( symbols >= 300 && symbols <= 500 ))'

# A table whose second symbol is gone.
awk 'BEGIN { rows = 0 } /^#/ { print; next } { rows++; if (rows != 2) print }' "$TABLE" \
    > "$SCRATCH/short.tsv"
run_status python3 -B "$SCRIPT" --check "$LIST" "$SCRATCH/short.tsv" "$FONT"
expect_status "a table without a listed symbol is stale" 1
expect_output "the stale table is named" "is not the table of"

# A table whose first symbol carries another keyword.
sed '3s/\t[^\t]*$/\tsomething/' "$TABLE" > "$SCRATCH/keywords.tsv"
run_status python3 -B "$SCRIPT" --check "$LIST" "$SCRATCH/keywords.tsv" "$FONT"
expect_status "a table with other keywords is stale" 1

# Two symbols on one code point.
awk -F '\t' 'BEGIN { OFS = "\t" } /^#/ { print; next } { if (++rows == 2) $2 = first; if (rows == 1) first = $2; print }' \
    "$TABLE" > "$SCRATCH/twice.tsv"
run_status python3 -B "$SCRIPT" --check "$LIST" "$SCRATCH/twice.tsv" "$FONT"
expect_status "two symbols on one code point are refused" 1
expect_output "the shared code point is named" "two symbols one code point"

# A font over the budget of 400 KiB.
head -c $((400 * 1024 + 1)) /dev/zero > "$SCRATCH/big.ttf"
run_status python3 -B "$SCRIPT" --check "$LIST" "$TABLE" "$SCRATCH/big.ttf"
expect_status "a font over its budget is refused" 1
expect_output "the budget is named" "more than 409600"

bad_list() {
    printf '%s\n' "$@" > "$SCRATCH/bad.txt"
    run_status python3 -B "$SCRIPT" --check "$SCRATCH/bad.txt" "$TABLE" "$FONT"
}
bad_list "toys folder"
expect_status "an unknown category does not read" 2
bad_list "files folder" "edit folder"
expect_status "a name listed twice does not read" 2
bad_list "files folder Directory"
expect_status "a keyword in upper case does not read" 2
bad_list "files"
expect_status "a line without a name does not read" 2

finish_tests icon-font
