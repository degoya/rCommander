#!/usr/bin/env bash
#
# scripts/ci-log.sh without the network (RC-100-31): a stand-in `gh` on PATH answers the run, its
# failed jobs and a job's raw log (a byte-order mark, colour codes, GitHub's timestamps). Checked:
# a run id and a job URL are read, the stored log is clean, only the lines that say what broke
# are printed with their line numbers, a shell step's failure gets the lines before its
# ##[error], a run without a failed job says so, and a wrong argument is refused.
#
# Pure bash, a second.
#
#   scripts/tests/ci-log.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
BIN="$SCRATCH/bin"
LOGS="$SCRATCH/logs"
mkdir -p "$BIN"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

# The answers: run 5 has the failed jobs in $SCRATCH/failed; job 7's log is a clippy failure,
# job 8's a failed download.
printf '\xEF\xBB\xBF2026-10-29T10:00:00.1234567Z \x1b[32mCompiling\x1b[0m rc-ops\n' > "$SCRATCH/log-7"
printf '%s\n' '2026-10-29T10:00:01.0000000Z error: unused variable: `x`' \
    '2026-10-29T10:00:02.0000000Z ##[error]Process completed with exit code 101.' >> "$SCRATCH/log-7"
printf '%s\n' '2026-10-29T10:00:00.0000000Z curl: (56) Recv failure' \
    '2026-10-29T10:00:01.0000000Z ##[error]Process completed with exit code 56.' > "$SCRATCH/log-8"
cat > "$BIN/gh" << 'SH'
#!/usr/bin/env bash
case "$*" in
    "api --allow-escape-sequences repos/"*"/actions/jobs/"*"/logs")
        id="${3%/logs}"; cat "$RC_FAKE_DIR/log-${id##*/}" ;;
    "api repos/"*"/actions/jobs/"*) printf '5\tfailure\tbuild (windows-2025)\n' ;;
    "run view 5 "*"status,conclusion"*) echo "completed failure https://github.com/o/r/actions/runs/5 CI" ;;
    "run view 5 "*"jobs"*) cat "$RC_FAKE_DIR/failed" ;;
    *) echo "unexpected gh $*" >&2; exit 9 ;;
esac
SH
chmod +x "$BIN/gh"

ci_log() {
    run_status env PATH="$BIN:$PATH" RC_FAKE_DIR="$SCRATCH" RC_CI_LOG_DIR="$LOGS" "$ROOT/scripts/ci-log.sh" "$@"
}

printf '7\tfailure\trust (windows-2025)\n8\tfailure\tchecks\n' > "$SCRATCH/failed"
ci_log 5
expect_status "a run with failed jobs exits 0" 0
expect_output "names how many failed" "failed jobs: 2"
expect_output "prints the compiler error with its line number" '2:error: unused variable: `x`'
expect_output "and GitHub's error line" "3:##[error]Process completed with exit code 101."
expect_output "a shell step's failure gets the lines before it" "1:curl: (56) Recv failure"
expect_true "the stored log is clean" \
    '[[ "$(head -n 1 "$LOGS/5-7.log")" == "Compiling rc-ops" ]]'

ci_log https://github.com/o/r/actions/runs/5/job/7
expect_status "a job URL reads that job" 0
expect_output "of its run" "job 7 of run 5 in o/r: failure"

: > "$SCRATCH/failed"
ci_log 5
expect_status "a run without a failed job" 0
expect_output "says so" "no failed job"

ci_log not-a-run
expect_status "a wrong argument is refused" 2

finish_tests ci-log
