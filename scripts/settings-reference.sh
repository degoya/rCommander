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
# Usage:
#   scripts/settings-reference.sh            # rewrite the page
#   scripts/settings-reference.sh --check    # exit 1 if the page is stale, 2 if the data does not read
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -z "${RC_WIKI:-}" ]]; then
    # The main checkout, also from a worktree: the parent of the common git directory.
    common="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2> /dev/null || echo "$ROOT/.git")"
    RC_WIKI="$(dirname "$(dirname "$common")")/rcommander.wiki"
fi
exec python3 -B "$ROOT/scripts/lib/settings_reference.py" "$ROOT" "$RC_WIKI" "$@"
