#!/usr/bin/env bash
#
# scripts/release-pipeline.sh end to end in a throwaway repository (RC-100-32,
# scripts/tests/lib/release-fixture.sh): --plan names the steps, in order, and what a run leaves
# out; a run without --push commits before it tests, merges into main, tags there the tree the
# test and the packages recorded, stages the public side locally and pushes nothing, and ends on
# development; a red step stops the chain without a tag and --resume continues from it, keeping
# the greens while the tree is unchanged and running them again after a change; a step that leaves
# something in the tree after the release commit, and a commit made beside the chain, are red at
# once (RC-110-31); a run of the earlier step order is not resumed; --push starts the public CI
# right after the commit, sends main, development and the tag to origin, and a red public CI holds
# the tag; after the tag a --resume publishes the unchanged tree and refuses a changed one; a
# pre-release is not merged into main and skips the wiki and the website.
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
# The ids of the green markers in the order they were written, on one line.
green_order() {
    sed -n 's/^##RC-STEP id=\([^ ]*\) .* exit=0 .*/\1/p' "$T/artifacts/release-evidence-$1.log" | paste -sd ' '
}
# The trees, begun and ended, of the markers of the steps $2... in the log of version $1, each once.
marker_trees() {
    local log="$T/artifacts/release-evidence-$1.log" id
    shift
    for id in "$@"; do
        grep "^##RC-STEP id=$id " "$log" | tail -n 1 | sed 's/.* from=\([^ ]*\) tree=\([^ ]*\).*/\1\n\2/'
    done | sort -u | paste -sd ' '
}

# --- the plan --------------------------------------------------------------------------------------
fresh plan
pipeline 9.9.9 --plan
expect_status "--plan exits 0" 0
expect_output "it counts the steps" "release 9.9.9: 17 steps, push disabled"
expect "the order: the documents and the commit before the test, the public side staged before the tag" \
    "preflight version-bump docs-gate archive-jobs commit public-ci-start test package-windows package-linux-x64 verify-artifacts merge-main public-ci stage-public evidence-gate tag push publish-public" \
    "$(sed -n 's/^  - \([a-z0-9-]*\).*/\1/p' <<< "$output" | paste -sd ' ')"
expect_output "the public CI's start needs --push" "public-ci-start (skipped: --push not given)"
expect_output "and so does its result" "public-ci (skipped: --push not given)"
expect_output "and so does push" "push (skipped: --push not given)"
expect_output "stage-public commits locally" "stage-public (committed locally, not pushed)"
expect_output "and nothing is published" "publish-public (skipped: --push not given)"
expect_true "and runs nothing" '[[ ! -e "$T/artifacts" ]]'
pipeline 9.9.9-beta.1 --plan --push
expect_output "a beta is named a pre-release" "release 9.9.9-beta.1 (pre-release): 17 steps, push enabled"
expect_output "and is not merged into main" "merge-main (skipped: a pre-release is not merged into main)"
expect_output "and is staged alone" "stage-public (the public export only: no wiki, no website)"
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
expect "every step but the public CI's two, push and publish-public left a marker" 13 "$(steps_in_log 9.9.9)"
expect "in the order of the plan: committed, then tested" \
    "preflight version-bump docs-gate archive-jobs commit test package-windows package-linux-x64 verify-artifacts merge-main stage-public evidence-gate tag" \
    "$(green_order 9.9.9)"
expect "the test, the packages and the gate ran on one tree, the tag's" "$(git -C "$T" rev-parse 'v9.9.9^{tree}')" \
    "$(marker_trees 9.9.9 commit test package-windows package-linux-x64 verify-artifacts merge-main stage-public evidence-gate)"
expect "the public export of HEAD, the wiki and the website staged, none with --push" \
    "export-public 9.9.9 --ref HEAD|export-wiki 9.9.9|update-website 9.9.9" "$(paste -sd '|' "$FAKE_CALLS")"
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

# A run begun while the chain still packaged Linux arm64 (until 2026-11-13) and stopped there: the
# step is gone, its red marker is a line nobody asks for, and the run goes on.
fresh arm64
touch "$FAKE_FAIL_CHECK"
pipeline 9.9.9
rm "$FAKE_FAIL_CHECK"
arm64_log="$T/artifacts/release-evidence-9.9.9.log"
printf '##RC-STEP id=package-linux-arm64 nonce=%s version=9.9.9 exit=1 bytes=5 started=x ended=y from=t tree=t\n' \
    "$(sed -n 's/^##RC-RELEASE .* nonce=\([^ ]*\) .*/\1/p' "$arm64_log")" >> "$arm64_log"
pipeline 9.9.9 --resume
expect_status "--resume of a run that stopped at the arm64 package passes" 0
expect_true "without building it" '! grep -qF "[package-linux-arm64]" <<< "$output"'
expect_true "and tags" 'git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'

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

# --- the tree after the release commit (RC-110-31) -------------------------------------------------------
fresh stray
touch "$FAKE_STRAY"
pipeline 9.9.9
expect_status "a step that leaves a file in the tree after the commit is red" 1
expect_output "naming the step" "[test] left the working tree"
expect_output "and the file" "stray.txt"
expect_true "the packages were not built on it" '! grep -q "^##RC-STEP id=package-windows " "$T/artifacts/release-evidence-9.9.9.log"'
expect_true "and there is no tag" '! git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
rm "$FAKE_STRAY" "$T/stray.txt"
pipeline 9.9.9 --resume
expect_status "--resume without the file passes" 0
expect "and the test ran again on the tagged tree" "$(git -C "$T" rev-parse 'v9.9.9^{tree}')" "$(marker_trees 9.9.9 test)"

fresh sneak
touch "$FAKE_SNEAK"
pipeline 9.9.9
expect_status "a commit made beside the chain is red" 1
expect_output "at the step it came in under" "[package-windows] began on tree"
expect_true "and there is no tag" '! git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
rm "$FAKE_SNEAK"
pipeline 9.9.9 --resume
expect_status "--resume passes" 0
expect_output "running the test again, whose green is of the other tree" \
    "[test] green in this run, but not on the tree HEAD carries - it runs again"
expect "so the test and the packages are on the tagged tree" "$(git -C "$T" rev-parse 'v9.9.9^{tree}')" \
    "$(marker_trees 9.9.9 test package-windows package-linux-x64 verify-artifacts)"
expect_true "which has the commit" 'git -C "$T" show v9.9.9:RELEASE-NOTES.md | grep -qx "sneaked in"'

# A log of the order before RC-110-31 - its header names no plan - is not resumed.
fresh oldplan
mkdir -p "$T/artifacts"
printf '##RC-RELEASE version=9.9.9 nonce=1-2-3 push=0 head=x branch=development started=s host=h\n##RC-STEP id=preflight nonce=1-2-3 version=9.9.9 exit=0 bytes=9 started=x ended=y tree=t\n' \
    > "$T/artifacts/release-evidence-9.9.9.log"
pipeline 9.9.9 --resume
expect_status "a run of the earlier step order is not resumed" 1
expect_output "saying why" "is a run of an earlier step order (plan 1, now 2)"
expect_output "and what to do" "Start the release again, without --resume"

# --- --push --------------------------------------------------------------------------------------------
fresh push
touch "$FAKE_FAIL_PUBLIC_CI"
pipeline 9.9.9 --push
expect_status "a red public CI stops the chain" 1
expect_output "before the tag" "[public-ci] failed with exit 1"
expect_output "its output in the evidence" "ci.yml: windows red"
expect "the CI was started once, on the release commit, beside the checks" \
    "public-ci ci/9.9.9 --ref $(git -C "$T" rev-parse development)" "$(paste -sd '|' "$FAKE_CALLS")"
expect "right after the commit, its result read after the merge" \
    "preflight version-bump docs-gate archive-jobs commit public-ci-start test package-windows package-linux-x64 verify-artifacts merge-main" \
    "$(green_order 9.9.9)"
expect_true "so there is no tag" '! git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
expect_true "and nothing reached origin" '[[ -z "$(git -C "$SCRATCH/push/origin.git" for-each-ref)" ]]'
expect "the checkout is back on development after the failure" development \
    "$(git -C "$T" rev-parse --abbrev-ref HEAD)"
rm "$FAKE_FAIL_PUBLIC_CI"
: > "$FAKE_CALLS"
pipeline 9.9.9 --resume --push
expect_status "--resume --push after the CI is green passes" 0
expect_output "going back onto main past merge-main" "merge-main is green in this run; back onto main"
expect "the CI ran again on the candidate's branch, the public side staged, then pushed" \
    "public-ci ci/9.9.9|export-public 9.9.9 --ref HEAD|export-wiki 9.9.9|update-website 9.9.9|export-public 9.9.9 --push|export-wiki 9.9.9 --push|update-website 9.9.9 --push" \
    "$(paste -sd '|' "$FAKE_CALLS")"
for ref in refs/heads/main refs/heads/development refs/tags/v9.9.9; do
    expect "origin has $ref" "$(git -C "$T" rev-parse "$ref")" \
        "$(git -C "$SCRATCH/push/origin.git" rev-parse "$ref" 2> /dev/null || echo missing)"
done

fresh green
pipeline 9.9.9 --push
expect_status "--push with a green public CI passes in one run" 0
expect "the CI on the release commit, then on HEAD, which its green covers" \
    "public-ci ci/9.9.9 --ref $(git -C "$T" rev-parse development)|public-ci ci/9.9.9" \
    "$(sed -n '1,2p' "$FAKE_CALLS" | paste -sd '|')"
expect_output "the background run's output is in the evidence" "--- the run started after the commit"

# After the tag: what is left only publishes, so the unchanged tree resumes and a changed one is refused.
fresh aftertag
touch "$FAKE_FAIL_PUBLISH"
pipeline 9.9.9 --push
expect_status "a refused public push stops the chain" 1
expect_output "after the tag" "[publish-public] failed with exit 1"
expect_true "which stands" 'git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
rm "$FAKE_FAIL_PUBLISH"
echo "an afterthought" >> "$T/RELEASE-NOTES.md"
pipeline 9.9.9 --resume --push
expect_status "--resume after the tag on a changed tree is refused" 1
expect_output "saying the tag stands" "v9.9.9 is tagged, and the tree is not the one the run left"
git -C "$T" checkout -q -- RELEASE-NOTES.md
: > "$FAKE_CALLS"
pipeline 9.9.9 --resume --push
expect_status "--resume after the tag on the unchanged tree passes" 0
expect_output "the tag standing" "[tag] already green in this run - skipped"
expect "only the publishing ran" "export-public 9.9.9 --push|export-wiki 9.9.9 --push|update-website 9.9.9 --push" \
    "$(paste -sd '|' "$FAKE_CALLS")"

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
expect "the public export alone, staged" "export-public 9.9.9-beta.1 --ref HEAD" "$(paste -sd '|' "$FAKE_CALLS")"

finish_tests release-pipeline
