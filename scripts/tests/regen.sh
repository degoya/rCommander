#!/usr/bin/env bash
#
# scripts/regen.sh and the list it shares with check.sh --preflight (scripts/lib/generators.sh),
# in a scratch tree with a stand-in for every script the list names: without an option each
# generator runs in its writing form, with --check in its --check form, a row written by hand is
# named and left alone, the wiki's rows wait for a wiki, every row runs after a failure and the
# exit says so. Then the real tree: every script of the list exists, check.sh takes its
# generator stages from the list, and ~/.local/bin gets on PATH (rc_user_tools_path).
#
# Pure bash, a second.
#
#   scripts/tests/regen.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=../lib/generators.sh
source "$ROOT/scripts/lib/generators.sh"
# shellcheck source=../lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"

TREE="$SCRATCH/tree"
mkdir -p "$TREE/scripts/lib" "$SCRATCH/wiki"
cp "$ROOT/scripts/regen.sh" "$TREE/scripts/"
cp "$ROOT/scripts/lib/generators.sh" "$TREE/scripts/lib/"
export CALLS="$SCRATCH/calls"
names=()
for row in "${RC_GENERATORS[@]}"; do
    rc_generator_fields "$row"
    names+=("$gen_name")
    expect_true "$gen_script exists and is executable" "[[ -x '$ROOT/$gen_script' ]]"
    expect_true "$gen_name has a --check form" "[[ '$gen_check' == *--check* ]]"
    cat > "$TREE/$gen_script" <<'STUB'
#!/usr/bin/env bash
echo "$(basename "$0") $*" >> "$CALLS"
[[ "$(basename "$0")" != "${FAIL_SCRIPT:-}" ]] || exit 3
STUB
    chmod +x "$TREE/$gen_script"
done
expect_true "the stage names are distinct" \
    '[[ "$(printf "%s\n" "${names[@]}" | sort -u | wc -l)" -eq "${#names[@]}" ]]'
called() { grep -cxF -- "$1" "$CALLS" || true; }

export RC_WIKI="$SCRATCH/wiki"
: > "$CALLS"
run_status "$TREE/scripts/regen.sh"
expect_status "regen.sh runs the generators" 0
expect "licenses.sh in its writing form" 1 "$(called 'licenses.sh ')"
expect "mcp-coverage.sh in its writing form" 1 "$(called 'mcp-coverage.sh ')"
expect "keyboard-reference.sh in its writing form" 1 "$(called 'keyboard-reference.sh ')"
expect "settings-reference.sh in its writing form" 1 "$(called 'settings-reference.sh ')"
expect "wit-reference.sh into the wiki" 1 "$(called "wit-reference.sh --wiki $SCRATCH/wiki")"
expect "nothing else ran" 5 "$(wc -l < "$CALLS")"
expect_output "a generator left to a hand is named" "symbol-font: not written here"
expect_output "with its reason" "manifest-defaults: not written here (by hand"

: > "$CALLS"
RC_WIKI="$SCRATCH/no-wiki" run_status "$TREE/scripts/regen.sh"
expect_status "without a wiki checkout" 0
expect_output "the wiki's row is named" "wit-reference: no wiki checkout at $SCRATCH/no-wiki"
expect "and not run" 0 "$(grep -c '^wit-reference' "$CALLS" || true)"

: > "$CALLS"
run_status "$TREE/scripts/regen.sh" --check
expect_status "regen.sh --check" 0
for row in "${RC_GENERATORS[@]}"; do
    rc_generator_fields "$row"
    expect "--check runs $gen_name as the list has it" 1 "$(called "$(basename "$gen_script") $gen_check")"
done
expect "every row once" "${#RC_GENERATORS[@]}" "$(wc -l < "$CALLS")"
expect_output "and says the files are current" "every generated file is current"

: > "$CALLS"
FAIL_SCRIPT=licenses.sh run_status "$TREE/scripts/regen.sh" --check
expect_status "a stale file: exit 1" 1
expect_output "named" "!! failed: licenses"
expect "the rows after it still ran" "${#RC_GENERATORS[@]}" "$(wc -l < "$CALLS")"

run_status "$TREE/scripts/regen.sh" --all
expect_status "an unknown argument: usage" 2

# check.sh takes the generators' stages from the list, not from lines of its own.
expect_true "check.sh loops over RC_GENERATORS" 'grep -qF "\"\${RC_GENERATORS[@]}\"" "$ROOT/scripts/check.sh"'
for row in "${RC_GENERATORS[@]}"; do
    rc_generator_fields "$row"
    expect "check.sh has no stage line of its own for $gen_name" 0 \
        "$(grep -c "^ *stage [a-z-]* $gen_script" "$ROOT/scripts/check.sh" || true)"
done

# The wiki's generated pages (RC-110-33): one checkout for every branch, so a stale page fails
# the --check only on main, development and an integration branch.
REPO="$SCRATCH/branches"
git init -q -b development "$REPO"
git -C "$REPO" -c user.name=test -c user.email=test@example.invalid commit -q --allow-empty -m base
unset RC_WIKI_STRICT
strict_on() {
    git -C "$REPO" checkout -q -B "$1"
    if rc_wiki_strict "$REPO"; then echo strict; else echo lenient; fi
}
expect "development is strict" strict "$(strict_on development)"
expect "main is strict" strict "$(strict_on main)"
expect "an integration branch is strict" strict "$(strict_on feature/integration-1x-w18)"
expect "a job's branch is lenient" lenient "$(strict_on feature/some-job)"
expect "a detached HEAD is lenient" lenient \
    "$(git -C "$REPO" checkout -q --detach; if rc_wiki_strict "$REPO"; then echo strict; else echo lenient; fi)"
git -C "$REPO" checkout -q -B feature/some-job
expect "RC_WIKI_STRICT=1 makes a job's branch strict" strict \
    "$(if RC_WIKI_STRICT=1 rc_wiki_strict "$REPO"; then echo strict; else echo lenient; fi)"
generator() { echo "ran $*"; return "$FAKE_STATUS"; }
FAKE_STATUS=1 run_status rc_wiki_generator "$REPO" generator --check
expect_status "a stale page on a job's branch passes the --check" 0
expect_output "with a notice that names who writes it" "by the coordinator"
FAKE_STATUS=2 run_status rc_wiki_generator "$REPO" generator --check
expect_status "data that does not fit fails there too" 2
FAKE_STATUS=1 run_status rc_wiki_generator "$REPO" generator
expect_status "the writing form's failure stays one" 1
FAKE_STATUS=0 run_status rc_wiki_generator "$REPO" generator --check
expect_true "a current page passes without a notice" '[[ $status -eq 0 && "$output" == "ran --check" ]]'
git -C "$REPO" checkout -q -B feature/integration-1x-w18
FAKE_STATUS=1 run_status rc_wiki_generator "$REPO" generator --check
expect_status "a stale page on an integration branch fails the --check" 1
git -C "$REPO" checkout -q -B development
RC_WIKI_STRICT=0 FAKE_STATUS=1 run_status rc_wiki_generator "$REPO" generator --check
expect_status "RC_WIKI_STRICT=0 lets it pass anywhere" 0
expect_true "keyboard-reference.sh and settings-reference.sh run through it" \
    'grep -q "^rc_wiki_generator " "$ROOT/scripts/keyboard-reference.sh" && grep -q "^rc_wiki_generator " "$ROOT/scripts/settings-reference.sh"'

# rc_user_tools_path: ~/.local/bin in front of PATH.
expect "a PATH without ~/.local/bin gets it first" "$SCRATCH/home/.local/bin:/usr/bin:/bin" \
    "$(HOME="$SCRATCH/home" PATH="/usr/bin:/bin"; rc_user_tools_path; echo "$PATH")"
expect "a PATH with it behind /usr/bin gets it first too" \
    "$SCRATCH/home/.local/bin:/usr/bin:$SCRATCH/home/.local/bin" \
    "$(HOME="$SCRATCH/home" PATH="/usr/bin:$SCRATCH/home/.local/bin"; rc_user_tools_path; echo "$PATH")"
expect "a PATH that starts with it is left alone" "$SCRATCH/home/.local/bin:/usr/bin" \
    "$(HOME="$SCRATCH/home" PATH="$SCRATCH/home/.local/bin:/usr/bin"; rc_user_tools_path; echo "$PATH")"
expect_true "check.sh and release-start.sh both call it" \
    'grep -qx "rc_user_tools_path" "$ROOT/scripts/check.sh" && grep -qx "rc_user_tools_path" "$ROOT/scripts/release-start.sh"'
expect "and no script sets the path itself" 0 \
    "$(grep -l 'HOME/.local/bin:\$PATH' "$ROOT"/scripts/*.sh | wc -l)"

finish_tests regen
