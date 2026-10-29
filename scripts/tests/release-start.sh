#!/usr/bin/env bash
#
# scripts/release-start.sh (RC-100-32): before it detaches anything it refuses, compiling
# nothing, a tree the chain would stop on - not on development, dirty, the tag taken, the
# changelog without the version's entries, the release notes without its section - naming each;
# a complete tree starts the pipeline detached, which writes its PID and, at the end, its exit
# code; --resume starts without the preconditions; a chain still running is not started twice.
# The pipeline is a stand-in that records its arguments, so no real chain can start from here.
#
#   scripts/tests/release-start.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test \
    GIT_COMMITTER_EMAIL=test@example.invalid GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset RC_RELEASE_BRANCH

BASE="$SCRATCH/base"
mkdir -p "$BASE/scripts/lib"
cp "$ROOT/scripts/release-start.sh" "$BASE/scripts/"
cp "$ROOT/scripts/lib/release-tag.sh" "$ROOT/scripts/lib/release-docs.sh" "$BASE/scripts/lib/"
cat > "$BASE/scripts/release-pipeline.sh" << 'EOF'
#!/usr/bin/env bash
echo "$*" > "$RC_RELEASE_RUN_DIR/pipeline-args"
exit "${FAKE_PIPELINE_EXIT:-0}"
EOF
chmod +x "$BASE/scripts/release-pipeline.sh"
printf '# Changelog\n\n## [Unreleased]\n\n- A thing.\n\n## [9.9.8] - 2026-10-01\n\n- Older.\n' > "$BASE/CHANGELOG.md"
printf '# Release notes\n\n## 9.9.9\n\n- Something a user notices.\n' > "$BASE/RELEASE-NOTES.md"
git init -q -b development "$BASE"
git -C "$BASE" add -A
git -C "$BASE" commit -qm base

# case <name> [shell run in the copy, then committed]: $tree is the copy, RC_RELEASE_RUN_DIR its
# run directory; release-start.sh is then started with the remaining arguments. A change that
# defines git() as a no-op stays uncommitted.
start_case() {
    local name="$1" change="$2"
    shift 2
    tree="$SCRATCH/$name"
    git clone -q -b development "$BASE" "$tree"
    (cd "$tree" && eval "$change" && { git add -A; git commit -qm change || true; })
    export RC_RELEASE_RUN_DIR="$SCRATCH/run-$name"
    run_status "$tree/scripts/release-start.sh" "$@"
}
# Waits up to ten seconds for the detached chain's exit file.
chain_exit() {
    local _
    for _ in $(seq 1 100); do
        [[ -f "$RC_RELEASE_RUN_DIR/exit" ]] && break
        sleep 0.1
    done
    cat "$RC_RELEASE_RUN_DIR/exit" 2> /dev/null || echo none
}

start_case form ':' 9.9
expect_status "a version that is no release version: usage" 2

start_case complete ':' 9.9.9 --push
expect_status "a complete tree starts the chain" 0
expect_output "naming its PID" "started, detached, as PID"
expect "the chain ends with the pipeline's exit" 0 "$(chain_exit)"
expect "and the pipeline got the version and --push" "9.9.9 --push" "$(cat "$RC_RELEASE_RUN_DIR/pipeline-args")"
expect_true "the chain's output is kept" 'grep -qF "chain ended with exit 0" "$RC_RELEASE_RUN_DIR/chain.log"'

export FAKE_PIPELINE_EXIT=5
start_case red ':' 9.9.9
expect "a red pipeline's exit is the chain's" 5 "$(chain_exit)"
unset FAKE_PIPELINE_EXIT

start_case dirty 'echo x > stray.txt; git() { :; }' 9.9.9
expect_status "a dirty tree: refused" 1
expect_output "naming the file" "stray.txt"
expect_true "and nothing started" '[[ ! -e "$RC_RELEASE_RUN_DIR/pipeline-args" ]]'

start_case branch 'git switch -q -c other' 9.9.9
expect_status "not on development: refused" 1
expect_output "saying where" "on 'other'"

start_case tagged 'git tag v9.9.9' 9.9.9
expect_status "the tag taken: refused" 1
expect_output "saying so" "tag v9.9.9 already exists"

start_case notes 'printf "# Release notes\n" > RELEASE-NOTES.md' 9.9.9
expect_status "no release notes for the version: refused" 1
expect_output "naming the file" "RELEASE-NOTES.md has no '## 9.9.9' section"

start_case unreleased-empty 'sed -i "/^- A thing.$/d" CHANGELOG.md' 9.9.9
expect_status "nothing to cut into the version: refused" 1
expect_output "naming the changelog" "nothing under '[Unreleased]' to cut into it"

start_case left-behind 'sed -i "s/^## \[9.9.8\]/## [9.9.9] - 2026-10-02\n\n- Done.\n\n## [9.9.8]/" CHANGELOG.md' 9.9.9
expect_status "entries beside an existing version section: refused" 1
expect_output "asking to move them" "move them into the version's section"

start_case resume 'echo x > stray.txt; git() { :; }' 9.9.9 --resume
expect_status "--resume starts without the preconditions" 0
expect "and hands --resume on" "9.9.9 --resume" "$(chain_exit > /dev/null; cat "$RC_RELEASE_RUN_DIR/pipeline-args")"

export RC_RELEASE_RUN_DIR="$SCRATCH/run-busy"
mkdir -p "$RC_RELEASE_RUN_DIR"
echo "$$" > "$RC_RELEASE_RUN_DIR/pid"
run_status "$SCRATCH/complete/scripts/release-start.sh" 9.9.9
expect_status "a chain still running is not started twice" 1
expect_output "naming it" "already running as PID $$"

finish_tests release-start
