#!/usr/bin/env bash
#
# Fuzzes the parsers of data from outside (RC-080-72): the cargo-fuzz targets in fuzz/ - FTP
# listings, known_hosts, archive listings, Total Commander's INI files, the terminal's OSC
# sequences, plugin manifests and the certificate viewer's files - each for a while with libFuzzer, on the nightly pinned in
# scripts/lib/tool-versions.sh (fuzz/ is a workspace of its own; the main one stays on stable).
#
# Usage:
#   scripts/fuzz.sh                       # every target, 60 s each
#   scripts/fuzz.sh --seconds 300         # every target, 5 min each (the nightly run)
#   scripts/fuzz.sh tc_ini osc            # only these
#   scripts/fuzz.sh --list                # the targets, one per line (takes no lock)
#
# Every target starts from the corpus it has grown in earlier runs plus the repository's test
# data for its parser (seeds(), copied in once). A finding is libFuzzer's input file under
# fuzz/artifacts/<target>/ (ignored by git); it becomes a test in the crate's tests/ with the
# fix (docs/development.md, "Fuzzing and coverage"). `cargo +<nightly> fuzz run <target>
# fuzz/artifacts/<target>/<file> --fuzz-dir fuzz` replays it.
#
# Holds the build lock for the whole run (scripts/lib/lock.sh - never wrap it in flock); the
# builds use RC_JOBS jobs (scripts/lib/jobs.sh).
#
# Environment:
#   RC_FUZZ_DIR        corpus and build (default target/fuzz, outside git and the repo lints):
#                      corpus/<target>/, build/
#   RC_FUZZ_LOGS       where the logs go (default /tmp/claude-<uid>/fuzz-<checkout>): one
#                      <time>-<pid>/ per run, a log per target and `summary`
#   RC_FUZZ_TOOLCHAIN  the nightly (default RC_FUZZ_NIGHTLY of tool-versions.sh)
#   RC_FUZZ_TIMEOUT    seconds one input may take before libFuzzer calls it a hang (default 25)
#
# Exit 0 when no target found anything; 1 when one found something or did not build or run;
# 2 on wrong arguments; 3 when the nightly or cargo-fuzz is not installed (nothing was run).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/fuzz/Cargo.toml"

# The targets: the [[bin]] names of fuzz/Cargo.toml, in its order.
list_targets() {
    awk '/^\[\[bin\]\]/ { bin = 1; next } /^\[/ { bin = 0 }
        bin && /^name = "/ { gsub(/^name = "|"$/, ""); print }' "$MANIFEST"
}

if [[ "$#" -eq 1 && "$1" == --list ]]; then
    list_targets
    exit 0
fi

# shellcheck source=lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# Sourced before the `cd`, because the lock library resolves this script's own path from $0.
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"
cd "$ROOT" || exit 1

usage() {
    echo "usage: scripts/fuzz.sh [<target>...] [--seconds N] | --list" >&2
    exit 2
}

seconds=60
targets=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seconds)
            [[ $# -ge 2 ]] || usage
            seconds="$2"
            shift 2
            ;;
        -*) usage ;;
        *)
            targets+=("$1")
            shift
            ;;
    esac
done
[[ "$seconds" =~ ^[1-9][0-9]*$ ]] || usage
TIMEOUT="${RC_FUZZ_TIMEOUT:-25}"
if [[ ! "$TIMEOUT" =~ ^[1-9][0-9]*$ ]]; then
    echo "RC_FUZZ_TIMEOUT must be a positive whole number of seconds, not '$TIMEOUT'" >&2
    exit 2
fi

mapfile -t known < <(list_targets)
if [[ "${#known[@]}" -eq 0 ]]; then
    echo "!! $MANIFEST names no target" >&2
    exit 2
fi
for target in "${targets[@]}"; do
    if [[ " ${known[*]} " != *" $target "* ]]; then
        echo "unknown target '$target'; the targets are: ${known[*]}" >&2
        exit 2
    fi
done
[[ "${#targets[@]}" -gt 0 ]] || targets=("${known[@]}")

TOOLCHAIN="${RC_FUZZ_TOOLCHAIN:-$RC_FUZZ_NIGHTLY}"
installed="$(rustup toolchain list 2> /dev/null)"
if [[ "$installed" != *"$TOOLCHAIN"* ]] || ! cargo "+$TOOLCHAIN" fuzz --version > /dev/null 2>&1; then
    echo "!! fuzzing needs $TOOLCHAIN and cargo-fuzz $RC_CARGO_FUZZ_VERSION; nothing was run:" >&2
    echo "   rustup toolchain install $TOOLCHAIN --profile minimal" >&2
    echo "   cargo install --locked cargo-fuzz --version $RC_CARGO_FUZZ_VERSION" >&2
    exit 3
fi
found="$(cargo "+$TOOLCHAIN" fuzz --version 2> /dev/null)"
if [[ "$found" != *" $RC_CARGO_FUZZ_VERSION"* ]]; then
    echo "note: $found is installed, scripts/lib/tool-versions.sh pins $RC_CARGO_FUZZ_VERSION" >&2
fi

FUZZ_DIR="${RC_FUZZ_DIR:-$ROOT/target/fuzz}"
LOG="${RC_FUZZ_LOGS:-/tmp/claude-$(id -u)/fuzz-$(basename "$ROOT")}/$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$LOG" "$FUZZ_DIR/build" || exit 1

# The files whose bytes start a target's corpus: the test data of its parser. The OSC target has
# none on disk; osc_seeds writes its seeds, everything_seeds those of Everything's answers.
seeds() {
    case "$1" in
        archive_listing) find crates/rc-vfs/tests/fixtures/archives -type f ! -name '*.py' ;;
        ftp_listing) find crates/rc-remote/tests/fixtures/ftp_list -type f ;;
        known_hosts) find crates/rc-remote/tests/fixtures/known_hosts -type f ;;
        plugin_manifest) find plugins -mindepth 2 -maxdepth 2 -name manifest.toml ;;
        tc_ini) find crates/rc-engine/tests/fixtures/total_commander -type f -iname '*.ini' ;;
        git_porcelain) find crates/rc-engine/tests/fixtures/git_porcelain -type f ;;
        cert_viewer) find crates/rc-cert/tests/fixtures -type f ;;
    esac 2> /dev/null | sort
}

# OSC 7, OSC 9;9 and OSC 133 as shells send them, between ordinary output (\134 is `\`).
osc_seeds() {
    printf 'out\033]7;file://localhost/home/me/src\007more' > "$1/seed-osc7"
    printf '\033]7;file://fuzz-host/tmp/a%%20b\033\134' > "$1/seed-osc7-host"
    printf '\033]9;9;"C:\134Users\134me"\033\134' > "$1/seed-osc9"
    printf '\033]133;A\007$ \033]133;B\007ls\033]133;C\007a b\n\033]133;D;0\007' > "$1/seed-osc133"
}

# Everything's answer to a search (`EVERYTHING_IPC_LIST2`) with the one path `C:\a`, as it
# arrives: five header words, one item (its flags and the offset 28), the length 4, the UTF-16
# text and its NUL (RC-100-93).
everything_seeds() {
    printf '\001\000\000\000\001\000\000\000\000\000\000\000\004\000\000\000\001\000\000\000' > "$1/seed-everything-one"
    printf '\000\000\000\000\034\000\000\000\004\000\000\000C\000:\000\134\000a\000\000\000' >> "$1/seed-everything-one"
}

# Copies the seeds into `corpus` once; a seed already there (by its path) is left alone.
seed() {
    local target="$1" corpus="$2" file name count=0
    if [[ "$target" == osc ]]; then
        [[ -e "$corpus/seed-osc7" ]] || osc_seeds "$corpus"
        return 0
    fi
    if [[ "$target" == everything_reply ]]; then
        [[ -e "$corpus/seed-everything-one" ]] || everything_seeds "$corpus"
        return 0
    fi
    while IFS= read -r file; do
        [[ -n "$file" ]] || continue
        name="seed-${file//\//_}"
        [[ -e "$corpus/$name" ]] || cp -- "$file" "$corpus/$name"
        count=$((count + 1))
    done < <(seeds "$target")
    [[ "$count" -gt 0 ]] || echo "note: no test data found to seed $target" >&2
}

# The main workspace's lock, so the targets fuzz the versions rCommander ships; cargo adds what
# the fuzz workspace needs on top (libfuzzer-sys) and drops the rest.
cp -- "$ROOT/Cargo.lock" "$ROOT/fuzz/Cargo.lock" || exit 1

echo "==> fuzzing ${targets[*]}: ${seconds} s each on $TOOLCHAIN (jobs: $JOBS, log: $LOG)"
failed=0
for target in "${targets[@]}"; do
    corpus="$FUZZ_DIR/corpus/$target"
    artifacts="$ROOT/fuzz/artifacts/$target"
    mkdir -p "$corpus" "$artifacts"
    seed "$target" "$corpus"
    # The artifacts there before the run: a finding is a file not among them (a time stamp
    # can equal the run's start on a coarse clock).
    before="$(find "$artifacts" -type f | sort)"
    echo "==> $target"
    CARGO_BUILD_JOBS="$JOBS" cargo "+$TOOLCHAIN" fuzz run --fuzz-dir "$ROOT/fuzz" \
        --target-dir "$FUZZ_DIR/build" --debug-assertions "$target" "$corpus" -- \
        -max_total_time="$seconds" -timeout="$TIMEOUT" -print_final_stats=1 \
        > "$LOG/$target.log" 2>&1 < /dev/null
    status=$?
    found="$(comm -13 <(printf '%s\n' "$before") <(find "$artifacts" -type f | sort) | sed '/^$/d' | head -n 1)"
    runs="$(sed -n 's/^stat::number_of_executed_units: *//p' "$LOG/$target.log" | tail -n 1)"
    if [[ "$status" -eq 0 ]]; then
        line="ok      $target: ${runs:-?} inputs in ${seconds} s"
    elif [[ -n "$found" ]]; then
        line="FINDING $target: $found ($LOG/$target.log)"
        failed=1
    else
        line="FAILED  $target: exit $status without a finding - the build or the run broke ($LOG/$target.log)"
        failed=1
    fi
    echo "$line" | tee -a "$LOG/summary"
done

if [[ "$failed" -eq 0 ]]; then
    echo "==> no finding in ${#targets[@]} targets"
else
    echo "!! a target found something or did not run: $LOG/summary"
fi
exit "$failed"
