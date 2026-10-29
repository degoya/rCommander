#!/usr/bin/env bash
#
# Writes or checks the generated contract reference (RC-100-12): every world, interface,
# function, record, variant and enum of crates/rc-plugin-api/wit/rcommander.wit, with its
# comments, as Markdown for the wiki's page for plugin authors (develop/plugin-development.md).
# The page carries it between `<!-- BEGIN wit-reference -->` and `<!-- END wit-reference -->`;
# nothing between the markers is edited by hand. The reader and the rules are in
# scripts/lib/wit-reference.py.
# Copied from rDownloader's scripts/wit-reference.sh at commit 26c8592a6.
#
# A contract change runs this in the same change as the WIT and the template copies
# (scripts/check-sdk-templates.sh holds those).
#
# Usage:
#   scripts/wit-reference.sh --check                                    # the WIT reads completely (preflight)
#   scripts/wit-reference.sh --print                                    # the Markdown to stdout
#   scripts/wit-reference.sh --wiki ~/projects/rcommander.wiki           # write the page
#   scripts/wit-reference.sh --wiki ~/projects/rcommander.wiki --check   # exit 1 when stale
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/scripts/lib/wit-reference.py" "$ROOT" "$@"
