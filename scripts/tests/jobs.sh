#!/usr/bin/env bash
#
# scripts/lib/jobs.sh, sourced in a subshell per case: the defaults, a value the caller set, the
# clippy jobs capped at the build jobs, the test threads at twice the jobs and never more than
# the machine's cores, and every value that is no positive whole number refused with exit 2.
#
# Pure bash: it runs in well under a second. check.sh --preflight runs it.
#
#   scripts/tests/jobs.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

unset RC_JOBS RC_CLIPPY_JOBS RC_TEST_THREADS
# The three values jobs.sh leaves, as `jobs clippy threads`, under the environment given.
values() {
    # shellcheck disable=SC2016  # expanded by the inner bash, after jobs.sh has set them
    env "$@" bash -c 'source "$1"; echo "$JOBS $CLIPPY_JOBS $TEST_THREADS"' _ "$ROOT/scripts/lib/jobs.sh"
}
cores="$(nproc 2> /dev/null || getconf _NPROCESSORS_ONLN)"
threads=8
[[ "$cores" -lt "$threads" ]] && threads="$cores"

expect "the defaults" "4 2 $threads" "$(values)"
expect "a caller's values win" "3 1 5" "$(values RC_JOBS=3 RC_CLIPPY_JOBS=1 RC_TEST_THREADS=5)"
expect "the clippy jobs never exceed the build jobs" "2 2" "$(values RC_JOBS=2 RC_CLIPPY_JOBS=6 | cut -d' ' -f1-2)"
threads=2
[[ "$cores" -lt "$threads" ]] && threads="$cores"
expect "the test threads are twice the jobs, capped at the cores" "$threads" "$(values RC_JOBS=1 | cut -d' ' -f3)"
for bad in RC_JOBS=0 RC_JOBS=four RC_CLIPPY_JOBS=-1 RC_TEST_THREADS=0; do
    run_status values "$bad"
    expect_status "$bad is refused" 2
done

finish_tests jobs
