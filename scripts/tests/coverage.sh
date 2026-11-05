#!/usr/bin/env bash
#
# scripts/coverage.sh in a throwaway repository, without cargo-llvm-cov: the script and its
# libraries copied, a stand-in cargo on PATH that records what it is asked and writes a small
# llvm-cov summary. Wrong arguments are refused; without cargo-llvm-cov nothing runs and the
# script says what to install; a run tests the workspace under coverage in RC_JOBS jobs and
# RC_TEST_THREADS threads, writes the HTML and the table per crate (crates and plugins, the
# registry left out, the total last); a failed test fails the run but the report is still
# written; scripts/lib/coverage.py refuses a file that is no llvm-cov export.
#
# Pure bash and python3: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/coverage.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
REPO="$SCRATCH/repo"
CALLS="$SCRATCH/calls"
OUT="$SCRATCH/coverage"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=../lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
has() { grep -qF -- "$2" "$1"; }

mkdir -p "$REPO/scripts/lib" "$SCRATCH/bin"
cp "$ROOT/scripts/coverage.sh" "$REPO/scripts/"
cp "$ROOT/scripts/lib/coverage.py" "$ROOT/scripts/lib/tool-versions.sh" "$ROOT/scripts/lib/jobs.sh" \
    "$ROOT/scripts/lib/lock.sh" "$REPO/scripts/lib/"

# An llvm-cov summary: two files of one crate, one of a plugin, one of the registry.
file() { printf '{"filename": "%s", "summary": {"lines": {"count": %d, "covered": %d}, "functions": {"count": %d, "covered": %d}}}' "$@"; }
{
    printf '{"type": "llvm.coverage.json.export", "version": "2.0.1", "data": [{"files": ['
    file "$REPO/crates/rc-a/src/lib.rs" 100 80 10 7
    printf ', '
    file "$REPO/crates/rc-a/src/x.rs" 50 40 4 1
    printf ', '
    file "$REPO/plugins/p-viewer/src/lib.rs" 20 5 2 2
    printf ', '
    file "/home/me/.cargo/registry/src/serde/lib.rs" 1000 0 100 0
    printf ']}]}\n'
} > "$SCRATCH/summary.json"

# cargo: records its arguments and the jobs; `llvm-cov --version` fails with FAKE_NO_COV, the
# tests exit FAKE_TEST_EXIT, the reports write what llvm-cov would.
cat > "$SCRATCH/bin/cargo" << EOF
#!/usr/bin/env bash
echo "cargo \$* jobs=\${CARGO_BUILD_JOBS:-} threads=\${NEXTEST_TEST_THREADS:-}" >> "$CALLS"
case "\$2" in
    --version) [[ -z "\${FAKE_NO_COV:-}" ]] || exit 101; echo "cargo-llvm-cov $RC_LLVM_COV_VERSION" ;;
    nextest) exit "\${FAKE_TEST_EXIT:-0}" ;;
    report)
        [[ "\$3" == --html ]] && { mkdir -p "\$5/html"; touch "\$5/html/index.html"; }
        [[ "\$3" == --json ]] && cp "$SCRATCH/summary.json" "\$6"
        ;;
esac
exit 0
EOF
chmod +x "$SCRATCH/bin/cargo"
export PATH="$SCRATCH/bin:$PATH" RC_COVERAGE_DIR="$OUT" RC_LOCK_FILE="$SCRATCH/build.lock" \
    RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1 RC_JOBS=3 RC_TEST_THREADS=5
unset RC_LOCK_HELD
COVERAGE="$REPO/scripts/coverage.sh"

run_status "$COVERAGE" --html
expect_status "an argument is refused" 2

FAKE_NO_COV=1 run_status "$COVERAGE"
expect_status "without cargo-llvm-cov nothing runs" 3
expect_output "and it says what to install" "cargo install --locked cargo-llvm-cov --version $RC_LLVM_COV_VERSION"
expect_true "no test was run" '! has "$CALLS" "llvm-cov nextest"'

rm -f "$CALLS"
run_status "$COVERAGE"
expect_status "a run with green tests passes" 0
expect_true "the workspace is tested under coverage in RC_JOBS jobs and RC_TEST_THREADS threads" \
    'has "$CALLS" "cargo llvm-cov nextest --locked --workspace --no-report --no-fail-fast jobs=3 threads=5"'
expect_true "the test files are left out of the report" 'has "$CALLS" "--ignore-filename-regex (_tests\.rs\$|/tests/)"'
expect_output "the crate's row adds its files" '| `rc-a` | 150 | 80.0 % | 14 | 57.1 % |'
expect_output "a plugin has a row" '| `p-viewer` | 20 | 25.0 % | 2 | 100.0 % |'
expect_output "the total leaves the registry out" '| **all** | 170 | 73.5 % | 16 | 62.5 % |'
expect_true "the table and the HTML are kept" '[[ -s "$OUT/crates.md" && -e "$OUT/html/index.html" ]]'

FAKE_TEST_EXIT=100 run_status "$COVERAGE"
expect_status "a failed test fails the run" 1
expect_output "but the report is still written" '| **all** | 170 | 73.5 % | 16 | 62.5 % |'

echo '{"type": "something else"}' > "$SCRATCH/other.json"
run_status python3 -B "$ROOT/scripts/lib/coverage.py" "$SCRATCH/other.json" "$REPO"
expect_status "a file that is no llvm-cov export is refused" 2
run_status python3 -B "$ROOT/scripts/lib/coverage.py" "$SCRATCH/missing.json" "$REPO"
expect_status "a missing file is refused" 2

finish_tests coverage
