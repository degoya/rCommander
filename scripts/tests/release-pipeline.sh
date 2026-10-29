#!/usr/bin/env bash
#
# scripts/release-pipeline.sh end to end in a throwaway repository (RC-100-32,
# scripts/tests/lib/release-fixture.sh): --plan names the steps and what a run leaves out; a run
# without --push merges into main, tags there, exports locally and pushes nothing, and ends on
# development; a red step stops the chain without a tag and --resume continues from it, keeping
# the greens while the tree is unchanged and running them again after a change; --push sends main,
# development and the tag to origin, and a red public CI holds the tag; a pre-release is not merged
# into main and skips the wiki and the website.
#
# Pure git, bash and python: it runs in seconds. check.sh --preflight runs it.
#
#   scripts/tests/release-pipeline.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=lib/release-fixture.sh
source "$ROOT/scripts/tests/lib/release-fixture.sh"

# A fresh fixture per case under $SCRATCH/<name>; $T is its tree.
fresh() {
    release_fixture "$SCRATCH/$1"
    T="$SCRATCH/$1/tree"
}
pipeline() { run_status "$T/scripts/release-pipeline.sh" "$@"; }
steps_in_log() { grep -c '^##RC-STEP ' "$T/artifacts/release-evidence-$1.log" || true; }

# --- the plan --------------------------------------------------------------------------------------
fresh plan
pipeline 9.9.9 --plan
expect_status "--plan exits 0" 0
expect_output "it counts the steps" "release 9.9.9: 16 steps, push disabled"
expect_output "public-ci needs --push" "public-ci (skipped: --push not given)"
expect_output "and so does push" "push (skipped: --push not given)"
expect_output "publish-public commits locally" "publish-public (committed locally, not pushed)"
expect_true "and runs nothing" '[[ ! -e "$T/artifacts" ]]'
pipeline 9.9.9-beta.1 --plan --push
expect_output "a beta is named a pre-release" "release 9.9.9-beta.1 (pre-release): 16 steps, push enabled"
expect_output "and is not merged into main" "merge-main (skipped: a pre-release is not merged into main)"
expect_output "and exports alone" "publish-public (the public export only: no wiki, no website)"
for bad in 9.9 v9.9.9 9.9.9-rc.1 9.9.9-beta.0; do
    pipeline "$bad" --plan
    expect_status "'$bad' is no release version" 2
done
pipeline 9.9.9 --skip-tests
expect_status "there is no --skip flag" 2

# --- a run without --push ----------------------------------------------------------------------------
fresh local
pipeline 9.9.9
expect_status "the run without --push passes" 0
expect_output "and says it pushed nothing" "9.9.9 released, not pushed"
expect_true "v9.9.9 is tagged" 'git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
expect "the tag is on main" "$(git -C "$T" rev-parse main)" "$(git -C "$T" rev-parse 'v9.9.9^{commit}')"
expect "the checkout is back on development" development "$(git -C "$T" rev-parse --abbrev-ref HEAD)"
expect "main is the merge of development" "$(git -C "$T" rev-parse development)" "$(git -C "$T" rev-parse main^2)"
expect "the release commit" "chore(release): 9.9.9" "$(git -C "$T" log -1 --format=%s development)"
expect_true "the bump cut the changelog" 'grep -qx "## \[9.9.9\] - 2026-10-29" "$T/CHANGELOG.md"'
expect_true "the tag message is the changelog section" \
    'git -C "$T" tag -l --format="%(contents)" v9.9.9 | grep -qxF -- "- A thing."'
expect_true "and keeps its heading" 'git -C "$T" tag -l --format="%(contents)" v9.9.9 | grep -qF "## [9.9.9] - "'
expect "every step but public-ci and push left a marker" 14 "$(steps_in_log 9.9.9)"
expect "the public export, the wiki and the website, none with --push" \
    "export-public 9.9.9|export-wiki 9.9.9|update-website 9.9.9" "$(paste -sd '|' "$FAKE_CALLS")"
expect_true "nothing reached origin" '[[ -z "$(git -C "$SCRATCH/local/origin.git" for-each-ref)" ]]'
expect_true "the evidence stays out of the commit" '! git -C "$T" ls-files --error-unmatch artifacts > /dev/null 2>&1'
pipeline 9.9.9
expect_status "a second run of the same version is refused" 1
expect_output "by the preflight" "tag v9.9.9 already exists"

# --- a red step and --resume ---------------------------------------------------------------------------
fresh resume
touch "$FAKE_FAIL_CHECK"
pipeline 9.9.9
expect_status "a red check.sh --full stops the chain with its exit" 3
expect_output "naming the step" "[test] failed with exit 3"
expect_output "and the way on" "scripts/release-pipeline.sh 9.9.9 --resume"
expect_true "without a tag" '! git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
rm "$FAKE_FAIL_CHECK"
pipeline 9.9.9 --resume
expect_status "--resume after the fix passes" 0
expect_output "keeping the preflight" "[preflight] already green in this run - skipped"
expect_output "and the bump, the tree being unchanged" "[version-bump] already green in this run - skipped"
expect_true "and tags" 'git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
pipeline 9.9.8 --resume
expect_status "--resume without a log of that version is refused" 1

fresh changed
touch "$FAKE_FAIL_CHECK"
pipeline 9.9.9
rm "$FAKE_FAIL_CHECK"
echo "a fix" >> "$T/RELEASE-NOTES.md"
pipeline 9.9.9 --resume
expect_status "--resume after a change of the tree passes" 0
expect_output "saying the tree changed" "the tree changed since the last step"
expect_output "keeping only the preflight" "[preflight] already green in this run - skipped"
expect_true "the bump runs again" '! grep -qF "[version-bump] already green" <<< "$output"'
expect_true "and the fix is in the release commit" 'git -C "$T" show v9.9.9:RELEASE-NOTES.md | grep -qx "a fix"'

# --- --push --------------------------------------------------------------------------------------------
fresh push
touch "$FAKE_FAIL_PUBLIC_CI"
pipeline 9.9.9 --push
expect_status "a red public CI stops the chain" 1
expect_output "before the tag" "[public-ci] failed with exit 1"
expect_true "so there is no tag" '! git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
expect_true "and nothing reached origin" '[[ -z "$(git -C "$SCRATCH/push/origin.git" for-each-ref)" ]]'
expect "the checkout is back on development after the failure" development \
    "$(git -C "$T" rev-parse --abbrev-ref HEAD)"
rm "$FAKE_FAIL_PUBLIC_CI"
: > "$FAKE_CALLS"
pipeline 9.9.9 --resume --push
expect_status "--resume --push after the CI is green passes" 0
expect_output "going back onto main past merge-main" "merge-main is green in this run; back onto main"
expect "the CI ran on the candidate's branch, then the export, wiki and website pushed" \
    "public-ci ci/9.9.9|export-public 9.9.9 --push|export-wiki 9.9.9 --push|update-website 9.9.9 --push" \
    "$(paste -sd '|' "$FAKE_CALLS")"
for ref in refs/heads/main refs/heads/development refs/tags/v9.9.9; do
    expect "origin has $ref" "$(git -C "$T" rev-parse "$ref")" \
        "$(git -C "$SCRATCH/push/origin.git" rev-parse "$ref" 2> /dev/null || echo missing)"
done

# --- a pre-release -------------------------------------------------------------------------------------
fresh beta
printf '\n## 9.9.9-beta.1\n\n- A first look.\n' >> "$T/RELEASE-NOTES.md"
git -C "$T" commit -qam notes
main_before="$(git -C "$T" rev-parse main)"
pipeline 9.9.9-beta.1
expect_status "a pre-release passes" 0
expect "main stays where it was" "$main_before" "$(git -C "$T" rev-parse main)"
expect "the tag is on development" "$(git -C "$T" rev-parse development)" \
    "$(git -C "$T" rev-parse 'v9.9.9-beta.1^{commit}')"
expect "the public export alone" "export-public 9.9.9-beta.1" "$(paste -sd '|' "$FAKE_CALLS")"

finish_tests release-pipeline
