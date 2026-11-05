#!/usr/bin/env bash
#
# The test coverage of the workspace (RC-080-72): every test of `check.sh --rust` once more under
# cargo-llvm-cov's instrumentation (nextest, stable with its llvm-tools component), then the HTML
# report and a table of line and function coverage per crate (scripts/lib/coverage.py) for
# docs/development.md ("Fuzzing and coverage"). No threshold: the numbers are measured, not a
# gate. The test files themselves (`*_tests.rs`, `tests/`) are left out of the numbers.
#
# Usage:
#   scripts/coverage.sh
#
# Holds the build lock for the whole run (scripts/lib/lock.sh - never wrap it in flock); builds
# with RC_JOBS jobs and runs RC_TEST_THREADS tests at once (scripts/lib/jobs.sh). The
# instrumented build goes to target/llvm-cov-target, beside the ordinary one.
#
# Environment:
#   RC_COVERAGE_DIR  where the report goes (default /tmp/claude-<uid>/coverage): html/index.html,
#                    summary.json (llvm-cov's), crates.md (the table, also printed), tests.log
#
# Exit 0 when every test passed and the report was written; 1 when a test failed (the report is
# still written) or the report could not be; 2 on wrong arguments; 3 when cargo-llvm-cov is not
# installed (nothing was run).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# Sourced before the `cd`, because the lock library resolves this script's own path from $0.
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"
cd "$ROOT" || exit 1

if [[ $# -gt 0 ]]; then
    echo "usage: scripts/coverage.sh" >&2
    exit 2
fi
if ! cargo llvm-cov --version > /dev/null 2>&1; then
    echo "!! coverage needs cargo-llvm-cov $RC_LLVM_COV_VERSION; nothing was run:" >&2
    echo "   cargo install --locked cargo-llvm-cov --version $RC_LLVM_COV_VERSION" >&2
    exit 3
fi
found="$(cargo llvm-cov --version 2> /dev/null)"
if [[ "$found" != *" $RC_LLVM_COV_VERSION"* ]]; then
    echo "note: $found is installed, scripts/lib/tool-versions.sh pins $RC_LLVM_COV_VERSION" >&2
fi

OUT="${RC_COVERAGE_DIR:-/tmp/claude-$(id -u)/coverage}"
mkdir -p "$OUT" || exit 1
rm -rf -- "$OUT/html" "$OUT/summary.json" "$OUT/crates.md"
IGNORE='(_tests\.rs$|/tests/)'

echo "==> the workspace's tests under coverage (jobs: $JOBS, test threads: $TEST_THREADS, log: $OUT/tests.log)"
CARGO_BUILD_JOBS="$JOBS" NEXTEST_TEST_THREADS="$TEST_THREADS" \
    cargo llvm-cov nextest --locked --workspace --no-report --no-fail-fast \
    > "$OUT/tests.log" 2>&1 < /dev/null
tests=$?
[[ "$tests" -eq 0 ]] || echo "!! a test failed (exit $tests): $OUT/tests.log; the report follows anyway"

echo "==> the report"
if ! cargo llvm-cov report --html --output-dir "$OUT" --ignore-filename-regex "$IGNORE" \
    > "$OUT/report.log" 2>&1 \
    || ! cargo llvm-cov report --json --summary-only --output-path "$OUT/summary.json" \
        --ignore-filename-regex "$IGNORE" >> "$OUT/report.log" 2>&1 \
    || ! python3 -B "$ROOT/scripts/lib/coverage.py" "$OUT/summary.json" "$ROOT" > "$OUT/crates.md"; then
    echo "!! the report was not written: $OUT/report.log" >&2
    exit 1
fi
cat "$OUT/crates.md"
echo "==> HTML: $OUT/html/index.html"
[[ "$tests" -eq 0 ]] || exit 1
exit 0
