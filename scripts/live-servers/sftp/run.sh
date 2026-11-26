#!/usr/bin/env bash
#
# The live SFTP tests (RC-080-14): starts the OpenSSH servers of ctl.sh in Docker, runs the
# opt-in suites crates/rc-engine/tests/live_sftp.rs and crates/rcommander/tests/live_mcp.rs
# (`rcommander mcp` headless) against them and removes every container and the network again -
# also after a failure or an interrupt. Not part of check.sh: it needs Docker and takes minutes.
#
# Usage:
#   scripts/live-servers/sftp/run.sh                 # every live test
#   scripts/live-servers/sftp/run.sh <filter>...     # nextest's filters, e.g. `resume` or `-E 'test(=x)'`
#
# Environment:
#   RC_LIVE_SFTP_PORT_BASE  the first of the thirteen ports on 127.0.0.1 (default 2201)
#   RC_LIVE_SFTP_BIG_MB     the size of the random file each server holds (default 300)
#   RC_LIVE_SFTP_KEEP=1     leave the servers running afterwards (ctl.sh down removes them)
#   RC_JOBS, RC_TEST_THREADS, and the lock's variables: see scripts/lib/jobs.sh and lock.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../../lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# shellcheck source=../../lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"
cd "$ROOT"

CTL="$ROOT/scripts/live-servers/sftp/ctl.sh"
command -v docker > /dev/null 2>&1 || { echo "run.sh: docker is not installed" >&2; exit 2; }

# shellcheck disable=SC2317,SC2329  # called by the trap
cleanup() {
    if [[ "${RC_LIVE_SFTP_KEEP:-0}" = 1 ]]; then
        echo "==> servers kept (RC_LIVE_SFTP_KEEP=1); remove them with: $CTL down"
    else
        "$CTL" down
        echo "==> servers removed"
    fi
}
trap cleanup EXIT

"$CTL" down
echo "==> starting the servers"
"$CTL" up

# Dummy credentials of the image (Dockerfile); the servers listen on 127.0.0.1 only.
export RC_LIVE_SFTP_CTL="$CTL"
export RC_LIVE_SFTP_HOST=127.0.0.1
export RC_LIVE_SFTP_USER=tester
export RC_LIVE_SFTP_PASSWORD=rc-live-dummy

# Optimised like a release (`release-test`): the tests print the times a person gets (the
# 100 000-entry listing, the transfers). `live_mcp` starts the `rcommander` binary as
# `rcommander mcp`, headless.
echo "==> cargo nextest run --locked --cargo-profile release-test -p rc-engine -p rcommander --test live_sftp --test live_mcp --run-ignored only (jobs $JOBS, threads $TEST_THREADS)"
status=0
CARGO_BUILD_JOBS="$JOBS" cargo nextest run --locked --cargo-profile release-test -p rc-engine \
    -p rcommander --test live_sftp --test live_mcp --run-ignored only \
    --no-fail-fast --success-output final --test-threads "$TEST_THREADS" "$@" || status=$?
exit "$status"
