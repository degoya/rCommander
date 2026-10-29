#!/usr/bin/env bash
#
# Writes the wiki's keyboard reference, reference/keyboard-shortcuts.md: every key of the keymap
# (crates/rc-engine/keymap/default.json) by its name, how the keyboard reaches each command
# without a key (crates/rcommander/src/app/palette/reach.json), the comparison with Total
# Commander (crates/rc-engine/keymap/tc.json) and the keys inside dialogs
# (crates/rc-engine/keymap/contexts.md).
#
# Generated, never kept by hand. It compiles nothing - scripts/lib/keyboard_reference.py reads
# the data - so --check runs in check.sh --preflight. The wiki is the checkout beside the main
# repository (../rcommander.wiki, found from a worktree too) or RC_WIKI; without one, --check
# still holds the data together and skips the page.
#
# Usage:
#   scripts/keyboard-reference.sh            # rewrite the page
#   scripts/keyboard-reference.sh --check    # exit 1 if the page is stale, 2 if the data does not fit
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -z "${RC_WIKI:-}" ]]; then
    # The main checkout, also from a worktree: the parent of the common git directory.
    common="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2> /dev/null || echo "$ROOT/.git")"
    RC_WIKI="$(dirname "$(dirname "$common")")/rcommander.wiki"
fi
exec python3 -B "$ROOT/scripts/lib/keyboard_reference.py" "$ROOT" "$RC_WIKI" "$@"
