# shellcheck shell=bash
#
# The generators of the repository, one list: scripts/regen.sh runs each one's writing form (or
# its --check form), and check.sh --preflight runs each one's --check form as a stage of the
# row's name. A new generated file gets a row here and nothing else.
#
#     source "$ROOT/scripts/lib/generators.sh"
#     for row in "${RC_GENERATORS[@]}"; do rc_generator_fields "$row"; ...; done
#
# A row: <stage>|<script>|<arguments that write>|<arguments that check>|<reason>
#   - `-` as the writing arguments: regen.sh does not write it, for the reason in the last field;
#     it is checked like the others.
#   - `@WIKI@` in the arguments is the wiki's checkout (rc_wiki_dir); regen.sh leaves out a form
#     that names it while there is none.
# The arguments are split on blanks.
#
# The wiki is one checkout for every branch and worktree, so a page generated there fits one
# branch at a time. rc_wiki_generator runs a wiki page's generator and lets a stale page fail
# its --check only where the page has to fit (rc_wiki_strict): on main, development and an
# integration branch. Anywhere else it is a notice - the coordinator writes the pages after the
# merge (scripts/regen.sh), never a job's branch (RC-110-33).

# shellcheck disable=SC2034  # read by the scripts that source this file
RC_GENERATORS=(
    "licenses|scripts/licenses.sh||--offline --check|"
    "mcp-coverage|scripts/mcp-coverage.sh||--check|"
    "keyboard-reference|scripts/keyboard-reference.sh||--check|"
    "settings-reference|scripts/settings-reference.sh||--check|"
    "wit-reference|scripts/wit-reference.sh|--wiki @WIKI@|--check|"
    "manifest-defaults|scripts/plugin-manifest-defaults.sh|-|--check|by hand: a manifest that loses a line needs its plugin's version raised"
    "symbol-font|scripts/icon-font.sh|-|--check|by hand: needs the network and fontTools, only when a glyph or a symbol changes"
)

# Takes one row apart into gen_name, gen_script, gen_write, gen_check and gen_reason.
rc_generator_fields() {
    # shellcheck disable=SC2034  # read by the caller
    IFS='|' read -r gen_name gen_script gen_write gen_check gen_reason <<< "$1"
}

# Whether a stale generated wiki page fails the --check in checkout $1: on `main`, `development`
# and `feature/integration-*`, the branches the pages are written for. RC_WIKI_STRICT=1 makes
# every branch strict, RC_WIKI_STRICT=0 none.
rc_wiki_strict() {
    local branch
    case "${RC_WIKI_STRICT:-}" in
        1) return 0 ;;
        0) return 1 ;;
    esac
    branch="$(git -C "$1" symbolic-ref --quiet --short HEAD 2> /dev/null)" || return 1
    case "$branch" in
        main | development | feature/integration-*) return 0 ;;
    esac
    return 1
}

# Runs the generator of a wiki page for checkout $1: the command $2... with its own exit code,
# but a --check that finds the page stale (exit 1; 2 is data that does not fit and stays) passes
# with a notice where the page need not fit this branch (rc_wiki_strict).
rc_wiki_generator() {
    local root="$1" status=0
    shift
    "$@" || status=$?
    if [[ "$status" -eq 1 && " $* " == *" --check "* ]] && ! rc_wiki_strict "$root"; then
        echo "note: the wiki's page does not fit this branch, which is no failure here: the one wiki" >&2
        echo "      checkout is written for development and the integration branch, by the coordinator" >&2
        echo "      after the merge (scripts/regen.sh). RC_WIKI_STRICT=1 makes it one." >&2
        return 0
    fi
    return "$status"
}

# The wiki's checkout for checkout $1: RC_WIKI, else rcommander.wiki beside the main checkout
# (found from a worktree too), as keyboard-reference.sh and settings-reference.sh find it.
rc_wiki_dir() {
    local common
    if [[ -n "${RC_WIKI:-}" ]]; then
        printf '%s\n' "$RC_WIKI"
        return 0
    fi
    common="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2> /dev/null || echo "$1/.git")"
    printf '%s\n' "$(dirname "$(dirname "$common")")/rcommander.wiki"
}
