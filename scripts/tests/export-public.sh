#!/usr/bin/env bash
#
# scripts/export-public.sh against a scratch "private" repository (RC-100-30): the export leaves
# out what scripts/public-exclude.txt names, commits "Release <version>" and tags it in the
# local clone and pushes nothing without --push; a link into a left-out path, a private name and
# a planted secret each refuse the export before anything reaches the clone; --branch takes only
# a ci/ branch; the version forms. RC-110-31: one export at a time per clone (a second one waits
# for the lock in the clone's .git, then refuses), a push only of the tree that was staged, and a
# release staged before its tag: pushed by a second call, replaced while it never left the
# machine, never moved once the public repository has it.
#
# Local only: the "public" repository is a bare repository in the scratch directory, gitleaks a
# stub that fails on a marker. When a real gitleaks is on PATH, one more case plants a key it
# knows. It runs in a few seconds.
#
#   scripts/tests/export-public.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

REAL_GITLEAKS="$(command -v gitleaks || true)"
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid
export GIT_CONFIG_COUNT=3 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=test
export GIT_CONFIG_KEY_1=user.email GIT_CONFIG_VALUE_1=test@example.invalid
export GIT_CONFIG_KEY_2=init.defaultBranch GIT_CONFIG_VALUE_2=main
export TMPDIR="$SCRATCH"
export RC_PUBLIC_DIR="$SCRATCH/public"
export RC_PUBLIC_REMOTE="$SCRATCH/public.git"
MARK="PLANTED-SECRET-""MARK"
export GITLEAKS="$SCRATCH/gitleaks"
printf '#!/bin/sh\n! grep -rqF "%s" .\n' "$MARK" > "$GITLEAKS"
chmod +x "$GITLEAKS"
git init -q --bare "$RC_PUBLIC_REMOTE"

# The private repository: the export and its guards, its own lists, and one file of each kind.
PRIVATE="$SCRATCH/private"
mkdir -p "$PRIVATE/scripts/lib" "$PRIVATE/docs" "$PRIVATE/src"
cp "$ROOT/scripts/export-public.sh" "$PRIVATE/scripts/"
cp "$ROOT/scripts/lib/public.sh" "$ROOT/scripts/lib/release-tag.sh" \
    "$ROOT/scripts/lib/public-links.py" "$ROOT/scripts/lib/public-names.py" \
    "$PRIVATE/scripts/lib/"
HOST="private.example""-host.test"
printf '%s\n' 'docs/' 'AGENTS.md' 'scripts/public-names.txt' > "$PRIVATE/scripts/public-exclude.txt"
printf '%s\n' "${HOST//./\\.}" > "$PRIVATE/scripts/public-names.txt"
printf '# none\n' > "$PRIVATE/scripts/public-names-allow.txt"
printf '# Project\n\nSee [the licence](LICENSE).\n' > "$PRIVATE/README.md"
printf 'GPL\n' > "$PRIVATE/LICENSE"
printf 'Internal notes; the host is %s.\n' "$HOST" > "$PRIVATE/docs/notes.md"
printf 'The working card.\n' > "$PRIVATE/AGENTS.md"
printf 'fn main() {}\n' > "$PRIVATE/src/main.rs"
git -C "$PRIVATE" init -q
git -C "$PRIVATE" add -A
git -C "$PRIVATE" commit -q -m base
git -C "$PRIVATE" tag v1.0.0

export_public() { run_status "$PRIVATE/scripts/export-public.sh" "$@"; }
in_clone() { git -C "$RC_PUBLIC_DIR" "$@"; }
remote_refs() { git -C "$RC_PUBLIC_REMOTE" for-each-ref --format='%(refname)' | tr '\n' ' '; }
# commit_private <file> <content>: one more commit in the private repository; prints its id.
commit_private() {
    mkdir -p "$(dirname "$PRIVATE/$1")"
    printf '%s\n' "$2" > "$PRIVATE/$1"
    git -C "$PRIVATE" add -A
    git -C "$PRIVATE" commit -q -m "$1"
    git -C "$PRIVATE" rev-parse HEAD
}
# refused <name> <needle> <version> <ref>: the export of <ref> fails, says why, and the clone stays.
refused() {
    local name="$1" needle="$2" before
    before="$(in_clone rev-parse HEAD)"
    export_public "$3" --ref "$4"
    expect_status "$name: refused" 1
    expect_output "$name: named" "$needle"
    expect "$name: the clone is untouched" "$before" "$(in_clone rev-parse HEAD)"
    expect "$name: nothing was pushed" "" "$(remote_refs)"
}

export_public 1.0.0
expect_status "a release is exported" 0
expect "the commit names the release" "Release 1.0.0" "$(in_clone log -1 --format=%s)"
expect "and is tagged" "$(in_clone rev-parse HEAD)" "$(in_clone rev-parse 'v1.0.0^{commit}')"
expect "the public tree" "LICENSE README.md scripts/export-public.sh scripts/lib/public-links.py scripts/lib/public-names.py scripts/lib/public.sh scripts/lib/release-tag.sh scripts/public-exclude.txt scripts/public-names-allow.txt src/main.rs " \
    "$(in_clone ls-files | tr '\n' ' ')"
expect_true "docs/, AGENTS.md and the private names are left out" \
    '[[ ! -e "$RC_PUBLIC_DIR/docs" && ! -e "$RC_PUBLIC_DIR/AGENTS.md" && ! -e "$RC_PUBLIC_DIR/scripts/public-names.txt" ]]'
expect "without --push nothing is pushed" "" "$(remote_refs)"
expect_output "and the way to publish is named" "not pushed. Publish with:"

export_public 1.0.0
expect_status "the same release again" 0
expect_output "is already done" "v1.0.0 is already exported with exactly this tree"

# The lists are the exported commit's (RC-110-33): a working tree that no longer leaves out docs/
# changes nothing for v1.0.0 - with the working tree's list docs/notes.md and its private name
# would be in the tree.
printf '%s\n' 'AGENTS.md' 'scripts/public-names.txt' > "$PRIVATE/scripts/public-exclude.txt"
export_public 1.0.0
expect_true "the exclude list is the exported commit's, not the working tree's" \
    '[[ $status -eq 0 ]] && grep -qF "already exported with exactly this tree" <<< "$output"'
git -C "$PRIVATE" checkout -q -- scripts/public-exclude.txt

# A fetch that fails is tried again, three times in all (RC-110-33): a git in front of PATH that
# fails the first FAKE_FETCH_FAILS fetches.
REAL_GIT="$(command -v git)"
mkdir -p "$SCRATCH/gitbin"
cat > "$SCRATCH/gitbin/git" << EOF
#!/usr/bin/env bash
if [[ " \$* " == *" fetch "* ]]; then
    count="\$(cat "$SCRATCH/fetches" 2> /dev/null || echo 0)"
    echo "\$((count + 1))" > "$SCRATCH/fetches"
    [[ "\$count" -ge "\${FAKE_FETCH_FAILS:-0}" ]] || exit 1
fi
exec "$REAL_GIT" "\$@"
EOF
chmod +x "$SCRATCH/gitbin/git"
PATH="$SCRATCH/gitbin:$PATH" FAKE_FETCH_FAILS=2 RC_PUBLIC_FETCH_PAUSE=0 export_public 1.0.0
expect_status "two failed fetches, then one that works: exported" 0
expect "after three fetches" 3 "$(cat "$SCRATCH/fetches")"
expect_output "each failure named" "fetch attempt 2 of 3 failed"
rm -f "$SCRATCH/fetches"
PATH="$SCRATCH/gitbin:$PATH" FAKE_FETCH_FAILS=3 RC_PUBLIC_FETCH_PAUSE=0 export_public 1.0.0
expect_status "three failed fetches: refused" 1
expect_output "saying so" "cannot fetch $RC_PUBLIC_REMOTE (three tries)"
expect "and not a fourth" 3 "$(cat "$SCRATCH/fetches")"

refused "a link into docs/" "README.md:3: links to docs" 1.0.1 \
    "$(commit_private README.md '# Project

See [the notes](docs/notes.md).')"
git -C "$PRIVATE" checkout -q v1.0.0 -- README.md

refused "a private name" "src/main.rs:1: names something private" 1.0.1 \
    "$(commit_private src/main.rs "// built on $HOST")"
git -C "$PRIVATE" checkout -q v1.0.0 -- src/main.rs

refused "a planted secret" "gitleaks found something in the export" 1.0.1 \
    "$(commit_private src/config.rs "const TOKEN: &str = \"$MARK\";")"
git -C "$PRIVATE" rm -q src/config.rs

if [[ -n "$REAL_GITLEAKS" ]]; then
    # An AWS access key id of nobody, built from pieces so this file holds none.
    key="AKIA""QZ4XR7TB2NW6LC3V"
    GITLEAKS="$REAL_GITLEAKS" refused "a key the real gitleaks knows" "gitleaks found something in the export" 1.0.1 \
        "$(commit_private src/aws.rs "let aws_access_key_id = \"$key\";")"
    git -C "$PRIVATE" rm -q src/aws.rs
else
    echo "skip the real gitleaks: not on PATH"
fi
git -C "$PRIVATE" commit -q -m "back to the release" --allow-empty

export_public 1.0.0 --push
expect_status "--push publishes" 0
expect "main and the tag reach the public repository" "refs/heads/main refs/tags/v1.0.0 " "$(remote_refs)"

# --- one export at a time (RC-110-31) ------------------------------------------------------------
# Another export holds the clone: the lock is an flock on a file in its .git. hold_lock <seconds>
# takes it in the background and returns once it is held; $holder is its PID.
lock="$RC_PUBLIC_DIR/.git/rc-export.lock"
hold_lock() {
    rm -f "$SCRATCH/held"
    (
        exec 9>> "$lock"
        flock 9
        : > "$SCRATCH/held"
        exec sleep "$1"
    ) &
    holder=$!
    for _ in $(seq 1 50); do
        [[ ! -e "$SCRATCH/held" ]] || break
        sleep 0.1
    done
}
hold_lock 60
before="$(in_clone rev-parse HEAD)"
RC_PUBLIC_LOCK_WAIT=1 export_public 1.0.1 --ref HEAD
expect_status "a second export into a clone another one works in: refused after the wait" 1
expect_output "naming the lock" "gave up after 1s waiting for $lock"
expect "the clone is untouched" "$before" "$(in_clone rev-parse HEAD)"
expect "and has no tag of it" "" "$(in_clone tag --list v1.0.1)"
kill "$holder"
wait "$holder" 2> /dev/null || true
RC_PUBLIC_LOCK_WAIT=soon export_public 1.0.0
expect_status "a wait that is no number is refused" 2
hold_lock 1
RC_PUBLIC_LOCK_WAIT=30 export_public 1.0.0
wait "$holder" 2> /dev/null || true
expect_status "an export waits for the one before it, then runs" 0
expect_output "saying what it waits for" "waiting for $lock"
expect_true "the lock is free when the export has ended" 'flock -n "$lock" true'

# --- staged before the tag, pushed after it (RC-110-31) ------------------------------------------
staged_from="$(commit_private src/lib.rs 'pub fn one() {}')"
export_public 1.0.2 --ref "$staged_from"
expect_status "a release is staged from a commit before its tag exists" 0
expect "one commit ahead of the public main" 1 "$(in_clone rev-list --count origin/main..main)"
expect "and nothing of it pushed" "refs/heads/main refs/tags/v1.0.0 " "$(remote_refs)"
restaged_from="$(commit_private src/lib.rs 'pub fn two() {}')"
export_public 1.0.2 --ref "$restaged_from"
expect_status "staged again after the tree changed" 0
expect_output "the commit that never left the machine is replaced" \
    "v1.0.2 was staged here with another tree and never pushed"
expect "still one commit ahead" 1 "$(in_clone rev-list --count origin/main..main)"
expect "the tag on it" "$(in_clone rev-parse HEAD)" "$(in_clone rev-parse 'v1.0.2^{commit}')"
expect "with the new tree" "pub fn two() {}" "$(in_clone show v1.0.2:src/lib.rs)"
git -C "$PRIVATE" tag v1.0.2 "$restaged_from"
export_public 1.0.2 --push
expect_status "after the tag the second call pushes what was staged" 0
expect_output "finding it" "v1.0.2 is already exported with exactly this tree"
expect "main and the tag are in the public repository" "$(in_clone rev-parse 'v1.0.2^{commit}')" \
    "$(git -C "$RC_PUBLIC_REMOTE" rev-parse refs/heads/main)"
export_public 1.0.2 --ref "$staged_from"
expect_status "a tag the public repository has is never moved" 1
expect_output "saying so" "v1.0.2 already exists in the public repository with a different tree"
expect "the clone stays on it" "$(in_clone rev-parse 'v1.0.2^{commit}')" "$(in_clone rev-parse HEAD)"

# Something writes into the clone between the commit and the push: nothing is pushed.
printf '#!/bin/sh\necho "from elsewhere" > stray.txt\n' > "$RC_PUBLIC_DIR/.git/hooks/post-commit"
chmod +x "$RC_PUBLIC_DIR/.git/hooks/post-commit"
refs_before="$(remote_refs)"
main_before="$(git -C "$RC_PUBLIC_REMOTE" rev-parse refs/heads/main)"
export_public 1.0.3 --ref "$staged_from" --push
expect_status "a clone that is not the committed tree any more: the push is refused" 1
expect_output "naming the ref" "refusing to push v1.0.3"
expect "no ref reached the public repository" "$refs_before" "$(remote_refs)"
expect "and its main stands" "$main_before" "$(git -C "$RC_PUBLIC_REMOTE" rev-parse refs/heads/main)"
rm "$RC_PUBLIC_DIR/.git/hooks/post-commit"
in_clone reset -q --hard
export_public 1.0.3 --ref "$staged_from" --push
expect_status "with the clone clean again the staged release is pushed" 0
expect "the tag is there" "$(in_clone rev-parse v1.0.3)" "$(git -C "$RC_PUBLIC_REMOTE" rev-parse refs/tags/v1.0.3)"

export_public 1.0.1 --branch ci/1.0.1
expect_status "--branch ci/<name> exports HEAD" 0
expect "and pushes only that branch" \
    "refs/heads/ci/1.0.1 refs/heads/main refs/tags/v1.0.0 refs/tags/v1.0.2 refs/tags/v1.0.3 " "$(remote_refs)"
expect "the clone is back on main" "main" "$(in_clone branch --show-current)"
expect "no tag for an unreleased export" "" "$(in_clone tag --list v1.0.1)"

export_public 1.0.1 --branch main
expect_status "--branch main is refused" 2
expect_output "saying which branches it takes" "--branch takes a ci/ branch: main"
export_public 1.0.1 --branch ci/x --push
expect_status "--branch with --push is refused" 2
export_public 1.0.1 --skip-push-ci
expect_status "--skip-push-ci without --branch is refused" 2

export_public 1.1.0-rc.1 --ref v1.0.0
expect_status "another pre-release form is refused" 2
expect_output "naming the forms" "X.Y.Z-beta.N for a pre-release"
export_public 1.1.0-beta.1 --ref refs/no/such-ref
expect_status "a beta is a release version" 1
expect_output "and gets as far as the ref" "no such commit in this repository: refs/no/such-ref"

GITLEAKS="$SCRATCH/missing" export_public 1.0.2 --ref v1.0.0
expect_status "no export without the scanner" 1
expect_output "naming it" "is not an executable file"

finish_tests export-public
