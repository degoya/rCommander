#!/usr/bin/env bash
#
# scripts/lib/lock.sh through a small locked script in a temp directory, on a lock file of its
# own: the re-exec runs the script under the lock with its arguments and exit code; a chain (the
# locked script starting itself again) holds one lock and does not deadlock; a run that cannot
# get the lock gives up with 199, one that cannot get the memory with 198; and RC_LOCK_HELD is
# trusted only from an ancestor that holds the lock - a stale `1`, a live PID that is no
# ancestor, or an ancestor while the lock is free all take the lock like any other run.
#
# bash and flock (util-linux); a second or two, most of it the two one-second waits. check.sh
# --preflight runs it.
#
#   scripts/tests/lock.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
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

if ! command -v flock > /dev/null 2>&1; then
    echo "skip: flock is not installed"
    finish_tests lock
    exit 0
fi

unset RC_LOCK_HELD RC_NO_LOCK
export RC_LOCK_FILE="$SCRATCH/build.lock" RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1 RC_LOCK_WAIT=2
LOCKED="$SCRATCH/locked.sh"
cat > "$LOCKED" <<EOF
#!/usr/bin/env bash
set -euo pipefail
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "\$@"
if flock -n "\$RC_LOCK_FILE" true; then echo "lock: free"; else echo "lock: held"; fi
echo "args: \$# [\${1:-}]"
case "\${1:-}" in
    chain) "\$0" inner ;;
    fail) exit 7 ;;
esac
echo "done: \${1:-}"
EOF
chmod +x "$LOCKED"

run_status "$LOCKED" "a b" c
expect_status "a locked script runs" 0
expect_output "it runs under the lock" "lock: held"
expect_output "its arguments survive the re-exec" "args: 2 [a b]"
run_status "$LOCKED" fail
expect_status "its exit code comes back" 7
run_status "$LOCKED" chain
expect_status "a chain does not deadlock" 0
expect_output "the inner script ran" "done: inner"
expect_true "the inner script did not wait" '! grep -qF "waiting for" <<< "$output"'

flock -o "$RC_LOCK_FILE" sleep 30 &
holder=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
    flock -n "$RC_LOCK_FILE" true 2> /dev/null || break
    sleep 0.1
done
RC_LOCK_WAIT=1 run_status "$LOCKED"
expect_status "a run that cannot get the lock gives up with 199" 199
expect_output "and says so" "gave up after 1s waiting for $RC_LOCK_FILE"
RC_LOCK_HELD=1 RC_LOCK_WAIT=1 run_status "$LOCKED"
expect_status "a stale RC_LOCK_HELD=1 does not skip the lock" 199
expect_output "and is named" "RC_LOCK_HELD=1 is no caller holding"
RC_LOCK_HELD="$holder" RC_LOCK_WAIT=1 run_status "$LOCKED"
expect_status "the PID of a holder that is no ancestor does not skip the lock" 199
release_holder

RC_LOCK_HELD=$$ run_status "$LOCKED"
expect_status "an ancestor while the lock is free: the run takes the lock" 0
expect_output "and runs under it" "lock: held"
expect_output "after saying why" "RC_LOCK_HELD=$$ is no caller holding"

RC_MIN_FREE_MB=lots run_status "$LOCKED"
expect_status "RC_MIN_FREE_MB that is no number is refused" 2
if [[ -r /proc/meminfo ]]; then
    RC_MIN_FREE_MB=999999999 RC_LOCK_WAIT=0 run_status "$LOCKED"
    expect_status "a run that cannot get the memory gives up with 198" 198
fi

finish_tests lock
