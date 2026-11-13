#!/usr/bin/env bash
#
# The evidence of scripts/release-pipeline.sh for what it refuses (RC-100-32): the gate passes
# only when every required step of *this* run has a passing, non-empty record, and every one from
# the release commit on began and ended on the tree HEAD carries (RC-110-31); a run of the earlier
# step order is refused; run_step reads a step's exit through the tee and counts a silent success
# as missing evidence; the commit refuses build output and key material in the index;
# scripts/tag-release.sh refuses without a run, with a red step, and on a tree the run did not
# record.
#
# The pipeline is sourced with RELEASE_PIPELINE_LIB=1 from the throwaway repository of
# scripts/tests/lib/release-fixture.sh, so its evidence log lands there.
#
#   scripts/tests/release-evidence.sh
# shellcheck disable=SC2016  # the snippets are single-quoted on purpose: the case's shell expands them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=lib/release-fixture.sh
source "$ROOT/scripts/tests/lib/release-fixture.sh"

release_fixture "$SCRATCH/lib"
T="$SCRATCH/lib/tree"
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
LOG="$T/artifacts/release-evidence-9.9.9.log"

# A case: a fresh run header from the sourced pipeline, every step before the gate recorded green
# - from the commit on with HEAD's tree, before it with another, as a run leaves them - then the
# snippet $1, in a shell of its own (a failed step's `exit` ends the case, not this script). $LOG,
# $NONCE, $HEAD_TREE and the step functions are there.
gate_case() {
    run_status bash -c 'set -euo pipefail
        cd "$1"
        RELEASE_PIPELINE_LIB=1 source scripts/release-pipeline.sh 9.9.9
        HEAD_TREE="$(git rev-parse "HEAD^{tree}")"
        for id in $(rc_release_required_before evidence-gate 0 0); do
            tree=before-the-commit
            ! rc_release_tree_bound "$id" || tree="$HEAD_TREE"
            printf "##RC-STEP id=%s nonce=%s version=9.9.9 exit=0 bytes=12 started=x ended=y from=%s tree=%s\n" \
                "$id" "$NONCE" "$tree" "$tree" >> "$LOG"
        done
        eval "$2"' _ "$T" "$1"
}

gate_case 'step_evidence_gate'
expect_status "every required step green and non-empty: the gate passes" 0
expect_output "and says so" "all 11 steps have passing, non-empty evidence"
gate_case 'sed -i "/id=test /s/ tree=.*/ tree=0123456789abcdef/" "$LOG"; step_evidence_gate'
expect_status "the test ended on another tree than HEAD carries: refused" 1
expect_output "naming the step and the tree" "test                 exit=0, but on tree"
expect_output "and the rule" "nothing is tagged that was not tested"
gate_case 'sed -i "/id=package-windows /s/ from=[^ ]*/ from=0123456789abcdef/" "$LOG"; step_evidence_gate'
expect_status "a package begun on another tree: refused" 1
expect_output "naming it" "package-windows      exit=0, but on tree 0123456789ab"
gate_case 'sed -i "/id=verify-artifacts /s/ from=[^ ]*//" "$LOG"; step_evidence_gate'
expect_status "a marker that names no tree it began on: refused" 1
gate_case 'echo more >> RELEASE-NOTES.md; git commit -qam "beside the chain"; step_evidence_gate'
expect_status "HEAD moved after the steps ran: refused" 1
expect_output "for every step of the old tree" "commit               exit=0, but on tree"
git -C "$T" reset -q --hard HEAD~1
gate_case 'sed -i "/^##RC-RELEASE/s/ plan=[0-9]*//" "$LOG"; step_evidence_gate'
expect_status "a run of the step order before RC-110-31: refused" 1
expect_output "saying so" "is a run of an earlier step order (plan 1, now 2)"
gate_case 'sed -i "/id=verify-artifacts /d" "$LOG"; step_evidence_gate'
expect_status "a step without a record: refused" 1
expect_output "naming it" "verify-artifacts     NO EVIDENCE"
gate_case 'sed -i "/id=test /s/exit=0/exit=1/" "$LOG"; step_evidence_gate'
expect_status "a red step: refused" 1
expect_output "with its exit" "test                 exit=1"
gate_case 'sed -i "/id=commit /s/bytes=12/bytes=0/" "$LOG"; step_evidence_gate'
expect_status "a green step without output: refused" 1
gate_case 'sed -i "/id=test /s/exit=0/exit=1/" "$LOG"
    printf "##RC-STEP id=test nonce=%s version=9.9.9 exit=0 bytes=5 started=x ended=y from=%s tree=%s\n" "$NONCE" "$HEAD_TREE" "$HEAD_TREE" >> "$LOG"
    step_evidence_gate'
expect_status "a red attempt fixed by a later green one: passes" 0
gate_case 'sed -i "s/nonce=$NONCE/nonce=other/" "$LOG"
    printf "##RC-RELEASE version=9.9.9 nonce=%s push=0 plan=2\n" "$NONCE" >> "$LOG"
    step_evidence_gate'
expect_status "the greens of another run: refused" 1
gate_case 'sed -i "/^##RC-RELEASE/d" "$LOG"; step_evidence_gate'
expect_status "a log without this run's header: refused" 1
gate_case 'printf "##RC-TREE-CHANGED nonce=%s was=a tree=b at=c\n" "$NONCE" >> "$LOG"; step_evidence_gate'
expect_status "greens from before a change of the tree: refused" 1
expect_output "but the preflight's stand" "preflight            exit=0, 12 bytes"
gate_case 'rc_release_gate preflight test public-ci'
expect_status "with --push the gate before the tag wants the public CI" 1
expect "the steps the tag needs without --push" \
    "preflight version-bump docs-gate archive-jobs commit test package-windows package-linux-x64 verify-artifacts merge-main stage-public evidence-gate" \
    "$(bash -c 'source "$1"; rc_release_required_before tag 0 0 | paste -sd " "' _ "$ROOT/scripts/lib/release-plan.sh")"
expect "a pre-release's gate leaves out the merge, and --push adds the public CI" \
    "verify-artifacts public-ci stage-public evidence-gate" \
    "$(bash -c 'source "$1"; rc_release_required_before tag 1 1 | sed -n "10,\$p" | paste -sd " "' _ "$ROOT/scripts/lib/release-plan.sh")"
expect "the steps held to the commit's tree" \
    "commit public-ci-start test package-windows package-linux-x64 verify-artifacts merge-main public-ci stage-public evidence-gate" \
    "$(bash -c 'source "$1"; for id in "${RC_RELEASE_STEPS[@]}"; do ! rc_release_tree_bound "$id" || echo "$id"; done | paste -sd " "' _ "$ROOT/scripts/lib/release-plan.sh")"

# run_step: the status through the tee, and output as part of the evidence.
gate_case 'failing() { echo "it broke"; return 7; }; run_step smoke failing'
expect_status "a red step stops with its own exit" 7
expect_output "naming it" "[smoke] failed with exit 7"
expect_true "and leaves a red marker" 'grep -q "^##RC-STEP id=smoke .* exit=7 " "$LOG"'
gate_case 'quiet() { :; }; run_step quiet quiet'
expect_status "a silent success is refused" 1
expect_output "as missing evidence" "exited 0 but produced no output"
gate_case 'RESUME=1; run_step test false'
expect_output "on --resume a green step is not run again" "[test] already green in this run - skipped"
gate_case 'RESUME=1; sed -i "/id=test /s/ tree=.*/ tree=0123456789abcdef/" "$LOG"; again() { echo "ran again"; }; run_step test again'
expect_output "unless its green is of another tree than HEAD carries" "[test] green in this run, but not on the tree HEAD carries - it runs again"
expect_output "then it runs" "ran again"
gate_case 'dirty() { echo "left by a step" > stray.txt; echo "wrote a file"; }; run_step test dirty'
expect_status "a step after the commit that leaves a file in the tree is red" 1
expect_output "naming the step" "[test] left the working tree"
expect_true "with a red marker" 'grep -q "^##RC-STEP id=test .* exit=1 .* tree=" "$LOG" && [[ "$(grep -c "^##RC-STEP id=test " "$LOG")" -eq 2 ]]'
rm -f "$T/stray.txt"
gate_case 'dirty() { echo "left by a step" > stray.txt; echo "wrote a file"; }; run_step version-bump dirty; rm stray.txt'
expect_status "before the commit a step may write into the tree" 0

# The commit takes nothing built and no key material.
gate_case 'echo x > plugin.rcplug; step_commit'
expect_status "a staged .rcplug is refused" 1
expect_output "by name" "plugin.rcplug"
git -C "$T" reset -q
rm -f "$T/plugin.rcplug"
gate_case 'echo x > signing.pem; step_commit'
expect_status "a staged key is refused" 1
git -C "$T" reset -q
rm -f "$T/signing.pem"

# tag-release.sh, standalone: no run, a red run, a tree the run did not record.
"$T/scripts/set-version.sh" 9.9.9 > /dev/null
git -C "$T" commit -qam "version 9.9.9"
rm -rf "$T/artifacts"
run_status "$T/scripts/tag-release.sh" 9.9.9
expect_status "without an evidence log: refused" 1
expect_output "saying so" "no evidence"
run_status "$T/scripts/tag-release.sh" 9.9.8
expect_status "another version than the workspace's: refused" 1
tag_case() {
    gate_case "printf '##RC-STEP id=evidence-gate nonce=%s version=9.9.9 exit=0 bytes=9 started=x ended=y from=%s tree=%s\n' \"\$NONCE\" \"$1\" \"$1\" >> \"\$LOG\"; $2"
    run_status "$T/scripts/tag-release.sh" 9.9.9
}
tag_case "$(git -C "$T" rev-parse 'HEAD^{tree}')" 'sed -i "/id=docs-gate /s/exit=0/exit=2/" "$LOG"'
expect_status "a red step in the run: the tag is refused" 1
expect_output "naming it" "docs-gate            exit=2"
tag_case 0123456789abcdef ':'
expect_status "a tree the run did not record: the tag is refused" 1
expect_output "saying which step" "evidence-gate        exit=0, but on tree 0123456789ab"
expect_output "and refusing" "refusing to tag v9.9.9"
tag_case "$(git -C "$T" rev-parse 'HEAD^{tree}')" 'sed -i "/id=test /s/ tree=.*/ tree=0123456789abcdef/" "$LOG"'
expect_status "a test recorded on another tree than HEAD's: the tag is refused" 1
expect_output "naming the test" "test                 exit=0, but on tree"
tag_case "$(git -C "$T" rev-parse 'HEAD^{tree}')" 'sed -i "/^##RC-RELEASE/s/ plan=[0-9]*//" "$LOG"'
expect_status "the evidence of the earlier step order: the tag is refused" 1
expect_output "saying so" "is a run of an earlier step order"
tag_case "$(git -C "$T" rev-parse 'HEAD^{tree}')" ':'
expect_status "this run's evidence for this tree: tagged" 0
expect_true "v9.9.9 is there" 'git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
run_status "$T/scripts/tag-release.sh" 9.9.9
expect_status "an existing tag is never moved" 1

finish_tests release-evidence
