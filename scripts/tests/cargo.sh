#!/usr/bin/env bash
#
# scripts/cargo.sh with a stand-in `cargo` on PATH and a lock file of its own: cargo runs under
# the build lock, in the caller's directory, with the arguments as they were and its exit code;
# the jobs come from scripts/lib/jobs.sh unless the caller set them; a script that holds the lock
# already can call it without waiting for itself; and while another run holds the lock it gives
# up with 199 and cargo never starts.
#
# bash and flock (util-linux); a second or two. check.sh --preflight runs it.
#
#   scripts/tests/cargo.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_SH="$ROOT/scripts/cargo.sh"
SCRATCH="$(mktemp -d)"
holder=""
release_holder() {
    [[ -n "$holder" ]] || return 0
    local child
    for child in $(ps -o pid= --ppid "$holder" 2> /dev/null); do kill "$child" 2> /dev/null || true; done
    kill "$holder" 2> /dev/null || true
    wait "$holder" 2> /dev/null || true
    holder=""
}
trap 'release_holder; rm -rf "$SCRATCH"' EXIT

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

run_status "$CARGO_SH"
expect_status "no arguments: usage" 2

if ! command -v flock > /dev/null 2>&1; then
    echo "skip: flock is not installed"
    finish_tests cargo
    exit 0
fi

unset RC_LOCK_HELD RC_NO_LOCK RC_JOBS RC_CLIPPY_JOBS RC_TEST_THREADS CARGO_BUILD_JOBS NEXTEST_TEST_THREADS
export RC_LOCK_FILE="$SCRATCH/build.lock" RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1 RC_LOCK_WAIT=2

mkdir -p "$SCRATCH/bin" "$SCRATCH/work dir"
cat > "$SCRATCH/bin/cargo" <<'STUB'
#!/usr/bin/env bash
if flock -n "$RC_LOCK_FILE" true; then echo "lock: free"; else echo "lock: held"; fi
echo "count: $#"
printf 'arg: [%s]\n' "$@"
echo "jobs: ${CARGO_BUILD_JOBS:-unset} threads: ${NEXTEST_TEST_THREADS:-unset}"
echo "cwd: $PWD"
[[ "${1:-}" != fail ]] || exit 7
STUB
chmod +x "$SCRATCH/bin/cargo"
export PATH="$SCRATCH/bin:$PATH"

cd "$SCRATCH/work dir"
RC_JOBS=3 RC_TEST_THREADS=5 run_status "$CARGO_SH" nextest run -p rc-process "a filter"
expect_status "cargo runs" 0
expect_output "under the build lock" "lock: held"
expect_output "with every argument" "count: 5"
expect_output "an argument with a blank in one piece" "arg: [a filter]"
expect_output "RC_JOBS as the build jobs, RC_TEST_THREADS as nextest's threads" "jobs: 3 threads: 5"
expect_output "in the caller's directory" "cwd: $SCRATCH/work dir"
expect_true "the lock is free afterwards" 'flock -n "$RC_LOCK_FILE" true'

run_status "$CARGO_SH" fail
expect_status "cargo's exit code comes back" 7

RC_JOBS=4 RC_CLIPPY_JOBS=2 run_status "$CARGO_SH" clippy --workspace --all-targets -- -D warnings
expect_output "a clippy over the workspace takes RC_CLIPPY_JOBS" "jobs: 2 "
RC_JOBS=4 RC_CLIPPY_JOBS=2 run_status "$CARGO_SH" clippy -p rc-vfs
expect_output "a clippy of one crate takes RC_JOBS" "jobs: 4 "
CARGO_BUILD_JOBS=1 NEXTEST_TEST_THREADS=9 run_status "$CARGO_SH" test
expect_output "the caller's own values win" "jobs: 1 threads: 9"
RC_JOBS=many run_status "$CARGO_SH" build
expect_status "RC_JOBS that is no number is refused" 2

# A script that holds the lock calls cargo.sh: one lock, no wait.
LOCKED="$SCRATCH/locked.sh"
printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' "source \"$ROOT/scripts/lib/lock.sh\"" \
    'rc_take_lock "$@"' "\"$CARGO_SH\" build --inner" > "$LOCKED"
chmod +x "$LOCKED"
run_status "$LOCKED"
expect_status "called from a script that holds the lock: no deadlock" 0
expect_output "cargo ran" "arg: [--inner]"
expect_output "under the caller's lock" "lock: held"
expect_true "without waiting" '! grep -qF "waiting for" <<< "$output"'

flock -o "$RC_LOCK_FILE" sleep 30 &
holder=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
    flock -n "$RC_LOCK_FILE" true 2> /dev/null || break
    sleep 0.1
done
RC_LOCK_WAIT=1 run_status "$CARGO_SH" build
expect_status "while another run holds the lock: gives up with 199" 199
expect_output "after waiting for it" "waiting for $RC_LOCK_FILE"
expect_true "and cargo never started" '! grep -qF "count:" <<< "$output"'
RC_NO_LOCK=1 run_status "$CARGO_SH" build
expect_status "RC_NO_LOCK=1 runs beside it" 0
release_holder

finish_tests cargo
