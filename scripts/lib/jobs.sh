# shellcheck shell=bash
#
# The parallelism of every script in scripts/, in one place.
#
# Usage, after ROOT is known:
#
#     source "$ROOT/scripts/lib/jobs.sh"
#
# Environment (a value that is already set wins):
#   RC_JOBS          cargo's build jobs (`-j`, CARGO_BUILD_JOBS). This bounds memory: the peak is
#                    in rustc and the link, one of each per job. Default 4.
#   RC_CLIPPY_JOBS   the build jobs of a clippy run over the whole workspace (`--workspace
#                    --all-targets`), which holds more rustc processes in memory at once than a
#                    build of one crate. Default 2, never more than RC_JOBS.
#   RC_TEST_THREADS  nextest's `--test-threads`. Default twice RC_JOBS, never more than the machine
#                    has cores: building is over by the time a test runs, and what a test thread
#                    costs is a core, not the memory RC_JOBS caps.
#
# Deliberately capped and not derived from nproc: letting cargo use every core of a WSL machine
# exhausts its memory and takes WSL down. The default moves only after a measurement.
#
# The values land in JOBS, CLIPPY_JOBS and TEST_THREADS. Not exported: a script that starts
# another passes them on explicitly, and a value the caller exported reaches every child anyway.

JOBS="${RC_JOBS:-4}"
CLIPPY_JOBS="${RC_CLIPPY_JOBS:-2}"
rc_jobs_cores="$(nproc 2> /dev/null || getconf _NPROCESSORS_ONLN 2> /dev/null || true)"
[[ "$rc_jobs_cores" =~ ^[1-9][0-9]*$ ]] || rc_jobs_cores=""

# A typo here would reach cargo as `-j` and fail late, or as 0 and mean something else.
for rc_jobs_name in JOBS CLIPPY_JOBS; do
    if [[ ! "${!rc_jobs_name}" =~ ^[1-9][0-9]*$ ]]; then
        echo "RC_$rc_jobs_name must be a positive whole number, not '${!rc_jobs_name}'" >&2
        exit 2
    fi
done
[[ "$CLIPPY_JOBS" -gt "$JOBS" ]] && CLIPPY_JOBS="$JOBS"

if [[ -n "${RC_TEST_THREADS:-}" ]]; then
    TEST_THREADS="$RC_TEST_THREADS"
    if [[ ! "$TEST_THREADS" =~ ^[1-9][0-9]*$ ]]; then
        echo "RC_TEST_THREADS must be a positive whole number, not '$TEST_THREADS'" >&2
        exit 2
    fi
else
    TEST_THREADS=$((JOBS * 2))
    if [[ -n "$rc_jobs_cores" && "$rc_jobs_cores" -lt "$TEST_THREADS" ]]; then
        TEST_THREADS="$rc_jobs_cores"
    fi
fi
unset rc_jobs_cores rc_jobs_name
