#!/usr/bin/env bash
#
# The checks of the repository, one mode per occasion. Every stage runs even after another one
# failed; the summary lists them all and the run ends with `REAL EXIT: <n>`, and with
# `==> all requested checks passed` only when every stage was green.
#
# Usage:
#   scripts/check.sh --preflight   # compiles nothing: cargo fmt --check, bash -n, shellcheck
#                                  # (at the pinned version), the script tests, gitleaks,
#                                  # git diff --check, the job layout, the file length,
#                                  # cargo deny --offline check, the migration pins,
#                                  # set-version.sh --check, licenses.sh --offline --check,
#                                  # mcp-coverage.sh --check,
#                                  # keyboard-reference.sh --check,
#                                  # settings-reference.sh --check, the plugin SDK's checks
#                                  # (check-sdk-templates.sh, wit-reference.sh --check,
#                                  # plugin-manifest-defaults.sh --check); no lock
#   scripts/check.sh --rust        # clippy over the workspace (-D warnings), every test
#                                  # (--no-fail-fast: every failing test is named), and clippy
#                                  # for wasm32-unknown-unknown over the plugins and the crates
#                                  # they share (lib only, in target/lanes/wasm32)
#   scripts/check.sh --windows     # cargo xwin clippy for x86_64-pc-windows-msvc (-D warnings),
#                                  # all targets, then the libraries alone with the features
#                                  # they ship with; in target/lanes/windows
#   scripts/check.sh --full        # all three (also the default without a mode)
#
# A green --full on a clean tree leaves a stamp, HEAD's commit in `$(git rev-parse --git-path
# rc-check-full)` of that checkout (per worktree); `scripts/worktree.sh finish` warns when the
# branch it merges has none for its tip. The tests run in nextest's profile NEXTEST_PROFILE
# (nextest's own variable; the nightly run, scripts/nightly.sh, sets `ci`, .config/nextest.toml).
#
# A tool a preflight stage needs (shellcheck, gitleaks, cargo-deny) that is not installed, or a
# ShellCheck other than the version scripts/lib/tool-versions.sh pins, is a visible skip under
# --preflight and a failure under --full, the gate.
#
# Logs: one directory per run, $RC_CHECK_LOGS/<time>-<pid>/ (default RC_CHECK_LOGS is
# /tmp/claude-<uid>/check-<checkout>), one log per stage plus `failures` and `summary`.
# Parallelism: scripts/lib/jobs.sh (RC_JOBS, RC_CLIPPY_JOBS, RC_TEST_THREADS). The compiling modes
# take the build lock (scripts/lib/lock.sh) - never wrap this script in flock.
# shellcheck disable=SC2317,SC2329  # the stage functions are called through stage(), not directly
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WINDOWS_TARGET="x86_64-pc-windows-msvc"
LIMIT=500

usage() {
    echo "usage: scripts/check.sh [--preflight|--rust|--windows|--full]" >&2
    exit 2
}

[[ $# -le 1 ]] || usage
mode="${1:---full}"
case "$mode" in
    --preflight | --rust | --windows | --full) ;;
    -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) usage ;;
esac

# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# shellcheck source=lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
if [[ "$mode" != --preflight ]]; then
    # shellcheck source=lib/lock.sh
    source "$ROOT/scripts/lib/lock.sh"
    rc_take_lock "$@"
fi
cd "$ROOT"

LOGS="${RC_CHECK_LOGS:-/tmp/claude-$(id -u)/check-$(basename "$ROOT")}/$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$LOGS"
: > "$LOGS/failures"
: > "$LOGS/summary"
echo "==> scripts/check.sh $mode in $ROOT (jobs $JOBS, clippy jobs $CLIPPY_JOBS, test threads $TEST_THREADS)"
echo "    logs: $LOGS"

failed=0
skipped=0

# The commit a green --full may stamp: HEAD, when the tree is clean as the run starts.
stamp_head=""
if [[ "$mode" == --full ]] && git rev-parse -q --verify HEAD > /dev/null \
    && [[ -z "$(git status --porcelain)" ]]; then
    stamp_head="$(git rev-parse HEAD)"
fi

# Runs one stage: `stage <name> <command...>`, its output into <name>.log. A failure is recorded
# and its log's end printed; the run goes on.
stage() {
    local name="$1" started status=0
    shift
    started="$(date +%s)"
    echo "--> $name"
    "$@" > "$LOGS/$name.log" 2>&1 || status=$?
    local took=$(($(date +%s) - started))
    if [[ "$status" -eq 0 ]]; then
        printf '  ok   %-14s %4ss\n' "$name" "$took" | tee -a "$LOGS/summary"
    else
        printf '  FAIL %-14s %4ss  exit %s  %s\n' "$name" "$took" "$status" "$LOGS/$name.log" \
            | tee -a "$LOGS/summary"
        echo "$name exit $status $LOGS/$name.log" >> "$LOGS/failures"
        echo "     --- last 30 lines of $name.log ---"
        tail -n 30 "$LOGS/$name.log" | sed 's/^/     /'
        failed=1
    fi
}

skip() {
    printf '  skip %-14s       %s\n' "$1" "$2" | tee -a "$LOGS/summary"
    skipped=$((skipped + 1))
}

# A stage whose tool is missing or not at its pinned version: a skip under --preflight alone, a
# failure under --full, so the gate never passes with a stage that did not run.
missing() {
    local name="$1" reason="$2"
    if [[ "$mode" == --full ]]; then
        printf '  FAIL %-14s       %s\n' "$name" "$reason" | tee -a "$LOGS/summary"
        echo "$name $reason" >> "$LOGS/failures"
        failed=1
    else
        skip "$name" "$reason (a failure under --full)"
    fi
}

# Every shell script of the repository, NUL-separated.
shell_scripts() {
    find scripts .claude/hooks -type f -name '*.sh' -print0 2> /dev/null
}

syntax() {
    local file status=0
    while IFS= read -r -d '' file; do
        bash -n "$file" || status=1
    done < <(shell_scripts)
    while IFS= read -r -d '' file; do
        python3 -c 'import ast, sys; ast.parse(open(sys.argv[1], encoding="utf-8").read(), sys.argv[1])' \
            "$file" || status=1
    done < <(find scripts .claude/hooks -type f -name '*.py' -print0 2> /dev/null)
    return "$status"
}

run_shellcheck() {
    local files=()
    while IFS= read -r -d '' file; do files+=("$file"); done < <(shell_scripts)
    shellcheck -x -P SCRIPTDIR "${files[@]}"
}

script_tests() {
    local test status=0
    for test in scripts/tests/*.sh; do
        [[ -e "$test" ]] || continue
        echo "==> $test"
        bash "$test" || status=1
    done
    return "$status"
}

# gitleaks over the tracked and the new (not ignored) files of the working tree, copied out and
# scanned from inside so that .gitleaks.toml applies and target/ is never read.
secret_scan() {
    local scanner="$1" copy status=0
    copy="$(mktemp -d)"
    git ls-files -z --cached --others --exclude-standard \
        | tar --null --ignore-failed-read -T - -cf - 2> /dev/null | tar -xf - -C "$copy" || status=$?
    if [[ "$status" -ne 0 || -z "$(find "$copy" -type f -print -quit)" ]]; then
        echo "!! copying the working tree for the scan failed (exit $status); nothing was scanned"
        rm -rf "$copy"
        return 1
    fi
    (cd "$copy" && "$scanner" dir . --no-banner --redact --exit-code 1) || status=$?
    rm -rf "$copy"
    [[ "$status" -eq 0 ]] || echo "!! a real secret is removed at the source; a fixture gets an entry in .gitleaks.toml"
    return "$status"
}

# Whitespace errors in the working tree against HEAD, or against the empty tree before the first
# commit, new files included. The new files are marked intent-to-add in a copy of the index, so
# the real one is never touched.
diff_check() {
    local base index status=0
    base="$(git rev-parse -q --verify HEAD || git hash-object -t tree /dev/null)"
    index="$(mktemp -u)"
    cp "$(git rev-parse --git-path index)" "$index" 2> /dev/null || true
    GIT_INDEX_FILE="$index" git add -A -N . || status=$?
    [[ "$status" -ne 0 ]] || GIT_INDEX_FILE="$index" git diff --check "$base" || status=$?
    rm -f "$index"
    return "$status"
}

# The 500-line rule for Rust sources and scripts, without compiling: the same rule as
# crates/rcommander/tests/repo_lints/file_length.rs, whose lists are empty. It reads the
# plugins and the SDK too, so a wave agent who compiles nothing still sees a long plugin file.
file_length() {
    local file lines status=0
    while IFS= read -r -d '' file; do
        lines="$(wc -l < "$file")"
        if [[ "$lines" -gt "$LIMIT" ]]; then
            echo "$file: $lines lines (limit $LIMIT; split it)"
            status=1
        fi
    done < <(find crates plugins sdk scripts -path '*/target' -prune -o -type f \
        \( -name '*.rs' -o -name '*.sh' -o -name '*.py' \) -print0 2> /dev/null)
    return "$status"
}

# Every migration matches its pin in crates/rc-db/migrations.sha384 and has one: what
# crates/rc-db/tests/migration_checksums.rs holds, without compiling. A missing pin is
# scripts/migration-pin.sh's; a changed file is restored, never re-pinned.
migration_pins() {
    local pins="crates/rc-db/migrations.sha384" file status=0
    (cd crates/rc-db/migrations && sha384sum --quiet -c "../$(basename "$pins")") || status=1
    for file in crates/rc-db/migrations/*.sql; do
        [[ -e "$file" ]] || continue
        if ! awk -v name="$(basename "$file")" '$2 == name { found = 1 } END { exit !found }' "$pins"; then
            echo "$file has no pin in $pins: scripts/migration-pin.sh"
            status=1
        fi
    done
    return "$status"
}

# The installed ShellCheck's version; scripts/lib/tool-versions.sh pins one, since another
# version reads the scripts differently.
shellcheck_version() {
    shellcheck --version | sed -n 's/^version: //p'
}

preflight() {
    echo "==> preflight"
    stage fmt cargo fmt --all --check
    stage syntax syntax
    if command -v shellcheck > /dev/null 2>&1; then
        stage shellcheck run_shellcheck
        local found
        found="$(shellcheck_version)"
        [[ "$found" == "$RC_SHELLCHECK_VERSION" ]] || missing shellcheck-version \
            "$found installed, $RC_SHELLCHECK_VERSION pinned (docs/development.md)"
    else
        missing shellcheck "not installed (docs/development.md)"
    fi
    stage script-tests script_tests
    local scanner
    scanner="${GITLEAKS:-$(command -v gitleaks || true)}"
    if [[ -n "$scanner" ]]; then
        stage gitleaks secret_scan "$scanner"
    else
        missing gitleaks "not installed (docs/development.md)"
    fi
    stage diff-check diff_check
    if [[ -x scripts/archive-jobs.sh ]]; then
        stage archive-jobs scripts/archive-jobs.sh --check
    else
        skip archive-jobs "scripts/archive-jobs.sh is missing"
    fi
    stage file-length file_length
    if cargo deny --version > /dev/null 2>&1; then
        stage deny cargo deny --offline check
    else
        missing deny "cargo-deny is not installed (docs/development.md)"
    fi
    stage migration-pins migration_pins
    stage version scripts/set-version.sh --check
    stage licenses scripts/licenses.sh --offline --check
    stage mcp-coverage scripts/mcp-coverage.sh --check
    stage keyboard-reference scripts/keyboard-reference.sh --check
    stage settings-reference scripts/settings-reference.sh --check
    stage sdk-templates scripts/check-sdk-templates.sh
    stage wit-reference scripts/wit-reference.sh --check
    stage manifest-defaults scripts/plugin-manifest-defaults.sh --check
}

rust() {
    echo "==> rust"
    stage clippy env CARGO_BUILD_JOBS="$CLIPPY_JOBS" \
        cargo clippy --locked --workspace --all-targets -j "$CLIPPY_JOBS" -- -D warnings
    stage tests env CARGO_BUILD_JOBS="$JOBS" \
        cargo nextest run --locked --workspace --no-fail-fast --test-threads "$TEST_THREADS"
    stage wasm32-clippy wasm_clippy
}

# The plugins' guest code (`cfg(target_arch = "wasm32")`, src/guest.rs) and the crates they share,
# for the target they ship on: nothing else here compiles it. Lib only; the tests run natively.
wasm_clippy() {
    local packages=(-p rc-plugin-types -p rc-plugin-text -p rc-plugin-api) manifest
    for manifest in plugins/*/Cargo.toml; do
        [[ -e "$manifest" ]] || continue
        packages+=(-p "$(sed -n '/^name = /{s/^name = "\(.*\)"/\1/p;q}' "$manifest")")
    done
    env CARGO_BUILD_JOBS="$CLIPPY_JOBS" CARGO_TARGET_DIR="$ROOT/target/lanes/wasm32" \
        cargo clippy --locked --target wasm32-unknown-unknown "${packages[@]}" -j "$CLIPPY_JOBS" \
        -- -D warnings
}

windows() {
    echo "==> windows"
    if ! cargo xwin --version > /dev/null 2>&1; then
        skip windows-clippy "cargo-xwin is not installed (docs/development.md)"
        failed=1
        echo "windows-clippy cargo-xwin missing" >> "$LOGS/failures"
        return 0
    fi
    stage windows-clippy env CARGO_BUILD_JOBS="$CLIPPY_JOBS" CARGO_TARGET_DIR="$ROOT/target/lanes/windows" \
        cargo xwin clippy --locked --target "$WINDOWS_TARGET" --workspace --all-targets \
        -j "$CLIPPY_JOBS" -- -D warnings
    # `--all-targets` builds the tests, which switches on the dev-dependencies' features
    # (`test-support`, `testkit`, `memory`, ...) in every library; the libraries as they ship are
    # linted only by this second run.
    stage windows-lib env CARGO_BUILD_JOBS="$CLIPPY_JOBS" CARGO_TARGET_DIR="$ROOT/target/lanes/windows" \
        cargo xwin clippy --locked --target "$WINDOWS_TARGET" --workspace -j "$CLIPPY_JOBS" \
        -- -D warnings
}

# The stamp of a green --full, for scripts/worktree.sh finish: only when the tree was clean from
# the start, HEAD is the commit it started on and nothing changed the tree on the way.
write_stamp() {
    local file
    if [[ -z "$stamp_head" ]]; then
        echo "    no green stamp: the tree had uncommitted changes when the run started"
        return 0
    fi
    if [[ "$(git rev-parse HEAD)" != "$stamp_head" || -n "$(git status --porcelain)" ]]; then
        echo "    no green stamp: HEAD or the tree changed during the run"
        return 0
    fi
    file="$(git rev-parse --path-format=absolute --git-path rc-check-full)"
    printf '%s %s %s\n' "$stamp_head" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$LOGS" > "$file"
    echo "    green stamp for ${stamp_head:0:12} in $file"
}

case "$mode" in
    --preflight) preflight ;;
    --rust) rust ;;
    --windows) windows ;;
    --full)
        preflight
        rust
        windows
        ;;
esac

echo
echo "==> summary ($mode)"
cat "$LOGS/summary"
if [[ "$failed" -eq 0 ]]; then
    [[ "$skipped" -eq 0 ]] || echo "    $skipped stage(s) skipped, see above"
    [[ "$mode" != --full || "$skipped" -ne 0 ]] || write_stamp
    echo "==> all requested checks passed"
else
    echo "!! failed stages (logs in $LOGS):"
    sed 's/^/   /' "$LOGS/failures"
fi
echo "REAL EXIT: $failed"
exit "$failed"
