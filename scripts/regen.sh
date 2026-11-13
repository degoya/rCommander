#!/usr/bin/env bash
#
# Every generated file of the repository in one run: the rows of scripts/lib/generators.sh, the
# list check.sh --preflight checks them by. After a merge into an integration branch it is
# `cargo fmt --all`, then this, then one `chore` commit (the skill `wave`).
#
# Usage:
#   scripts/regen.sh            # run every generator that writes; names what is left to a hand
#   scripts/regen.sh --check    # run every generator's --check form, as the preflight does
#
# Every row runs even after another one failed; exit 1 when one did. It compiles nothing and
# takes no lock. The keyboard, settings and contract references are pages of the wiki's checkout
# (RC_WIKI, else rcommander.wiki beside the main checkout): what changed there is committed
# there. licenses.sh may ask the network for the manifests of another platform's crates.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

check=0
case "$#:${1:-}" in
    0:) ;;
    1:--check) check=1 ;;
    1:-h | 1:--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) echo "usage: scripts/regen.sh [--check]" >&2; exit 2 ;;
esac

# shellcheck source=lib/generators.sh
source "$ROOT/scripts/lib/generators.sh"
cd "$ROOT"
wiki="$(rc_wiki_dir "$ROOT")"

failed=()
for row in "${RC_GENERATORS[@]}"; do
    rc_generator_fields "$row"
    form="$gen_write"
    [[ "$check" -eq 0 ]] || form="$gen_check"
    if [[ "$form" == - ]]; then
        echo "--  $gen_name: not written here ($gen_reason)"
        continue
    fi
    if [[ "$form" == *@WIKI@* && ! -d "$wiki" ]]; then
        echo "--  $gen_name: no wiki checkout at $wiki (RC_WIKI)"
        continue
    fi
    arguments=()
    read -r -a arguments <<< "$form"
    arguments=("${arguments[@]//@WIKI@/$wiki}")
    echo "==> $gen_script ${arguments[*]}"
    status=0
    "$gen_script" "${arguments[@]}" || status=$?
    if [[ "$status" -ne 0 ]]; then
        echo "!! $gen_name: exit $status"
        failed+=("$gen_name")
    fi
done

echo
if [[ "${#failed[@]}" -gt 0 ]]; then
    echo "!! failed: ${failed[*]}"
    exit 1
fi
if [[ "$check" -eq 1 ]]; then
    echo "==> every generated file is current"
else
    echo "==> regenerated; commit what git status shows here and in $wiki"
fi
