#!/usr/bin/env bash
#
# Writes the wiki's settings reference, reference/settings.md: every row of the settings schema
# (SETTINGS in crates/rc-engine/src/settings/schema.rs), page by page in the settings dialog's
# order, with its key, values, default, help text and whether it is protected or needs a restart.
#
# Generated, never kept by hand. It compiles nothing - scripts/lib/settings_reference.py reads
# the Rust source - so --check runs in check.sh --preflight. The wiki is the checkout beside the
# main repository (../rcommander.wiki, found from a worktree too) or RC_WIKI; without one,
# --check still reads the data and skips the page.
#
# The wiki is one checkout for all branches: a stale page fails --check on main, development and
# an integration branch, and is a notice on every other one (scripts/lib/generators.sh,
# rc_wiki_generator; RC_WIKI_STRICT=1|0 decides it by hand). The coordinator writes the page
# after a merge (scripts/regen.sh).
#
# Usage:
#   scripts/settings-reference.sh            # rewrite the page
#   scripts/settings-reference.sh --check    # exit 1 if the page is stale, 2 if the data does not read
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/generators.sh
source "$ROOT/scripts/lib/generators.sh"
status=0
rc_wiki_generator "$ROOT" python3 -B "$ROOT/scripts/lib/settings_reference.py" "$ROOT" "$(rc_wiki_dir "$ROOT")" "$@" \
    || status=$?
exit "$status"
