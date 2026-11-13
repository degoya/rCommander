#!/usr/bin/env bash
#
# cargo under the build lock: the way to run a cargo command that compiles (build, check, clippy,
# test, nextest, run, doc, bench) by hand, so it never runs beside a heavy script or another
# checkout's build. The arguments go to cargo as they are, cargo runs in the caller's directory
# and its exit code comes back.
#
# Usage:
#   scripts/cargo.sh nextest run -p rc-process <filter>     # the smallest check while working
#   scripts/cargo.sh clippy -p rc-vfs --all-targets -- -D warnings
#   scripts/cargo.sh xwin build -p rcommander --target x86_64-pc-windows-msvc
#
# The lock, the memory threshold and sccache are scripts/lib/lock.sh's (RC_LOCK_FILE,
# RC_LOCK_WAIT, RC_MIN_FREE_MB, RC_NO_LOCK; exit 199 without the lock, 198 without the memory),
# so never wrap this in flock. Called from a script that holds the lock already it runs under
# that lock (RC_LOCK_HELD) and does not wait for itself.
#
# The parallelism is scripts/lib/jobs.sh's unless the caller set it: CARGO_BUILD_JOBS is RC_JOBS
# (RC_CLIPPY_JOBS for a clippy over the whole workspace), NEXTEST_TEST_THREADS is
# RC_TEST_THREADS. A `-j` or `--test-threads` among the arguments wins, as cargo has it.
#
# `cargo fmt`, `metadata`, `tree`, `update` and `deny` compile nothing and need no lock; they
# pass through here all the same. The Bash guard (.claude/hooks/guard-bash.py) refuses the
# compiling subcommands typed as bare `cargo` and names this script.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

case "${1:-}" in
    "") echo "usage: scripts/cargo.sh <cargo arguments...>" >&2; exit 2 ;;
    -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
esac

# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"

build_jobs="$JOBS"
clippy=0
workspace=0
for argument in "$@"; do
    case "$argument" in
        clippy) clippy=1 ;;
        --workspace) workspace=1 ;;
        --) break ;;
    esac
done
[[ "$clippy" -eq 0 || "$workspace" -eq 0 ]] || build_jobs="$CLIPPY_JOBS"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$build_jobs}"
export NEXTEST_TEST_THREADS="${NEXTEST_TEST_THREADS:-$TEST_THREADS}"
exec cargo "$@"
