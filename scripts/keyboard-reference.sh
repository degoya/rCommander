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
# The wiki is one checkout for all branches: a stale page fails --check on main, development and
# an integration branch, and is a notice on every other one (scripts/lib/generators.sh,
# rc_wiki_generator; RC_WIKI_STRICT=1|0 decides it by hand). The coordinator writes the page
# after a merge (scripts/regen.sh).
#
# Usage:
#   scripts/keyboard-reference.sh            # rewrite the page
#   scripts/keyboard-reference.sh --check    # exit 1 if the page is stale, 2 if the data does not fit
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/generators.sh
source "$ROOT/scripts/lib/generators.sh"
status=0
rc_wiki_generator "$ROOT" python3 -B "$ROOT/scripts/lib/keyboard_reference.py" "$ROOT" "$(rc_wiki_dir "$ROOT")" "$@" \
    || status=$?
exit "$status"
