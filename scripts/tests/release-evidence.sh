#!/usr/bin/env bash
#
# The evidence of scripts/release-pipeline.sh for what it refuses (RC-100-32): the gate passes
# only when every required step of *this* run has a passing, non-empty record; run_step reads a
# step's exit through the tee and counts a silent success as missing evidence; the commit refuses
# build output and key material in the index; scripts/tag-release.sh refuses without a run, with a
# red step, and on a tree the run did not record.
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

# A case: a fresh run header from the sourced pipeline, every step before the gate recorded green,
# then the snippet $1, in a shell of its own (a failed step's `exit` ends the case, not this
# script). $LOG, $NONCE and the step functions are there.
gate_case() {
    run_status bash -c 'set -euo pipefail
        cd "$1"
        RELEASE_PIPELINE_LIB=1 source scripts/release-pipeline.sh 9.9.9
        for id in $(rc_release_required_before evidence-gate 0 0); do
            printf "##RC-STEP id=%s nonce=%s version=9.9.9 exit=0 bytes=12 started=x ended=y tree=t\n" "$id" "$NONCE" >> "$LOG"
        done
        eval "$2"' _ "$T" "$1"
}

gate_case 'step_evidence_gate'
expect_status "every required step green and non-empty: the gate passes" 0
expect_output "and says so" "all 11 steps have passing, non-empty evidence"
gate_case 'sed -i "/id=verify-artifacts /d" "$LOG"; step_evidence_gate'
expect_status "a step without a record: refused" 1
expect_output "naming it" "verify-artifacts     NO EVIDENCE"
gate_case 'sed -i "/id=test /s/exit=0/exit=1/" "$LOG"; step_evidence_gate'
expect_status "a red step: refused" 1
expect_output "with its exit" "test                 exit=1"
gate_case 'sed -i "/id=commit /s/bytes=12/bytes=0/" "$LOG"; step_evidence_gate'
expect_status "a green step without output: refused" 1
gate_case 'sed -i "/id=test /s/exit=0/exit=1/" "$LOG"
    printf "##RC-STEP id=test nonce=%s version=9.9.9 exit=0 bytes=5 started=x ended=y tree=t\n" "$NONCE" >> "$LOG"
    step_evidence_gate'
expect_status "a red attempt fixed by a later green one: passes" 0
gate_case 'sed -i "s/nonce=$NONCE/nonce=other/" "$LOG"
    printf "##RC-RELEASE version=9.9.9 nonce=%s push=0\n" "$NONCE" >> "$LOG"
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
    "preflight version-bump test package-windows package-linux-x64 package-linux-arm64 verify-artifacts docs-gate archive-jobs commit merge-main evidence-gate" \
    "$(bash -c 'source "$1"; rc_release_required_before tag 0 0 | paste -sd " "' _ "$ROOT/scripts/lib/release-plan.sh")"
expect "a pre-release's gate leaves out the merge" \
    "commit evidence-gate public-ci" \
    "$(bash -c 'source "$1"; rc_release_required_before tag 1 1 | sed -n "10,\$p" | paste -sd " "' _ "$ROOT/scripts/lib/release-plan.sh")"

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
    gate_case "printf '##RC-STEP id=evidence-gate nonce=%s version=9.9.9 exit=0 bytes=9 started=x ended=y tree=%s\n' \"\$NONCE\" \"$1\" >> \"\$LOG\"; $2"
    run_status "$T/scripts/tag-release.sh" 9.9.9
}
tag_case "$(git -C "$T" rev-parse 'HEAD^{tree}')" 'sed -i "/id=docs-gate /s/exit=0/exit=2/" "$LOG"'
expect_status "a red step in the run: the tag is refused" 1
expect_output "naming it" "docs-gate            exit=2"
tag_case 0123456789abcdef ':'
expect_status "a tree the run did not record: the tag is refused" 1
expect_output "saying which" "HEAD's tree is not the one [evidence-gate] recorded"
tag_case "$(git -C "$T" rev-parse 'HEAD^{tree}')" ':'
expect_status "this run's evidence for this tree: tagged" 0
expect_true "v9.9.9 is there" 'git -C "$T" rev-parse -q --verify refs/tags/v9.9.9 > /dev/null'
run_status "$T/scripts/tag-release.sh" 9.9.9
expect_status "an existing tag is never moved" 1

finish_tests release-evidence
