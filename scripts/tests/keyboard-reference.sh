#!/usr/bin/env bash
#
# scripts/lib/keyboard_reference.py against a copy of the keymap data in a temp directory: the
# page is written into a temp wiki with the rows the keymap gives (a snapshot of its shape), --check
# passes on it, a renamed action makes --check fail as stale, and data that does not fit is refused
# with exit 2 - a command without a key and without a reach row, a reach row for a command with
# a key, a name without its words, a key named by a place no keyboard has (a key by place,
# `Ctrl+[Semicolon]`, reads as a German and a US keyboard print it). Without a wiki - none given, none beside the checkout -
# --check skips the page with exit 0 and still checks the data.
#
# Pure python3 and bash: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/keyboard-reference.sh
# shellcheck disable=SC2016  # the condition is single-quoted on purpose: expect_true evaluates it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/lib/keyboard_reference.py"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
WIKI="$SCRATCH/wiki"
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
PAGE="$WIKI/reference/keyboard-shortcuts.md"
KEYMAP="$TREE/crates/rc-engine/keymap/default.json"
REACH="$TREE/crates/rcommander/src/app/palette/reach.json"
CATALOGUE="$TREE/crates/rcommander/locales/en.json"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$TREE/crates/rc-engine/src/registry" "$TREE/crates/rc-engine/keymap" \
    "$(dirname "$REACH")" "$(dirname "$CATALOGUE")" "$WIKI"
cp "$ROOT/crates/rc-engine/src/registry.rs" "$TREE/crates/rc-engine/src/registry.rs"
cp "$ROOT"/crates/rc-engine/src/registry/*.rs "$TREE/crates/rc-engine/src/registry/"
cp "$ROOT"/crates/rc-engine/keymap/*.json "$TREE/crates/rc-engine/keymap/"
cp "$ROOT/crates/rcommander/src/app/palette/reach.json" "$REACH"
cp "$ROOT/crates/rcommander/locales/en.json" "$CATALOGUE"
cp "$ROOT/crates/rc-engine/keymap/contexts.md" "$TREE/crates/rc-engine/keymap/"
# The keymap keeps no reserved key now; one is added so the page's reserved section is tested.
sed -i '0,/^\[$/s//[\n  { "keys": "Ctrl+Alt+Shift+F11", "reserved": "a later job (script test)" },/' \
    "$TREE/crates/rc-engine/keymap/default.json"

run_status python3 -B "$SCRIPT" "$TREE" "$WIKI"
expect_status "the page is written" 0
expect_true "a name with all its keys" 'grep -qF "| Ctrl+R, F2 | Reread the panel | \`panel.refresh\` |" "$PAGE"'
expect_true "a key with parameters by its own name" 'grep -qF "| Ctrl+F3 | Sort by name (again: reverse) | \`panel.sort\` |" "$PAGE"'
expect_true "a window key" 'grep -qF "| Ctrl+Shift+P | Command palette | window |" "$PAGE"'
expect_true "the toggle leaves a terminal" 'grep -qF "| Switch between file list and terminal (also in a terminal) |" "$PAGE"'
expect_true "a key by place reads as on a German and a US keyboard" 'grep -qF "| Ctrl+Ö / Ctrl+;, Ctrl+^ / Ctrl+\\\` | Switch between" "$PAGE"'
expect_true "a reserved key" 'grep -qF "| Ctrl+Shift+Alt+F11 | a later job (script test) |" "$PAGE"'
expect_true "a command through a dialog" 'grep -qF "| \`hotlist.open\` | An entry of the hotlist (Ctrl+D) (\`hotlist.list\`) |" "$PAGE"'
expect_true "a palette command" 'grep -qF "| \`vault.lock\` | Command palette: *Lock the vault* |" "$PAGE"'
expect_true "a TC key left free" 'grep -qF "| Ctrl+F7 | Unsorted | Not planned yet (later) |" "$PAGE"'
expect_true "the dialogs' keys follow" 'grep -qF "### Lister (F3) and Quick View (Ctrl+Q)" "$PAGE"'
expect_true "the leading comment is dropped" '! grep -qF "<!--" "$PAGE"'
expect_true "the counts add up" 'grep -qE "^[0-9]+ commands: [0-9]+ with a key" "$PAGE"'
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "--check passes on a current page" 0

sed -i 's/"keys.panel.refresh": "Reread the panel"/"keys.panel.refresh": "Read the panel again"/' "$CATALOGUE"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "--check fails on a stale page" 1
expect_output "--check names the script" "run scripts/keyboard-reference.sh"
cp "$ROOT/crates/rcommander/locales/en.json" "$CATALOGUE"

sed -i '/"command": "vault.lock"/d' "$REACH"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a command without key and row is refused" 2
expect_output "the command is named" "vault.lock has no key and no row"
cp "$ROOT/crates/rcommander/src/app/palette/reach.json" "$REACH"

sed -i 's/^\[$/[\n  { "keys": "F11", "command": "vault.lock" },/' "$KEYMAP"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a row for a bound command is refused" 2
expect_output "the row is named" "vault.lock has a key: drop its row"
cp "$ROOT/crates/rc-engine/keymap/default.json" "$KEYMAP"

sed -i 's/^\[$/[\n  { "keys": "Ctrl+[Nowhere]", "window": "cmdline_history" },/' "$KEYMAP"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a key by an unknown place is refused" 2
expect_output "the place is named" "no key is named [Nowhere]"
cp "$ROOT/crates/rc-engine/keymap/default.json" "$KEYMAP"

sed -i 's/^\[$/[\n  { "keys": "F11", "window": "zz_unknown" },/' "$KEYMAP"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a name without its words is refused" 2
expect_output "the key is named" "no keys.zz_unknown"
cp "$ROOT/crates/rc-engine/keymap/default.json" "$KEYMAP"

run_status python3 -B "$SCRIPT" "$TREE" "$SCRATCH/no-wiki" --check
expect_status "without a wiki --check checks the data only" 0
expect_output "the skip is said" "the page is not checked"
run_status python3 -B "$SCRIPT" "$TREE" "$SCRATCH/no-wiki"
expect_status "without a wiki nothing is written" 2

# The wrapper without RC_WIKI, in a checkout with no wiki beside it (a CI): --check skips the page
# with a notice and exit 0; the data is still checked.
mkdir -p "$TREE/scripts/lib"
cp "$ROOT/scripts/keyboard-reference.sh" "$TREE/scripts/"
cp "$ROOT/scripts/lib/keyboard_reference.py" "$ROOT/scripts/lib/mcp_coverage.py" "$TREE/scripts/lib/"
git init -q "$TREE"
run_status env -u RC_WIKI "$TREE/scripts/keyboard-reference.sh" --check
expect_status "no wiki beside the checkout: --check passes" 0
expect_output "the skip names the place looked at" "no wiki at $SCRATCH/rcommander.wiki"
sed -i '/"command": "vault.lock"/d' "$REACH"
run_status env -u RC_WIKI "$TREE/scripts/keyboard-reference.sh" --check
expect_status "no wiki, but the data is still checked" 2
cp "$ROOT/crates/rcommander/src/app/palette/reach.json" "$REACH"
mkdir -p "$SCRATCH/rcommander.wiki"
run_status env -u RC_WIKI "$TREE/scripts/keyboard-reference.sh" --check
expect_status "a wiki beside the checkout without the page is stale" 1

finish_tests keyboard-reference
