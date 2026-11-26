#!/usr/bin/env bash
#
# Fuzzes the parsers of data from outside (RC-080-72): the cargo-fuzz targets in fuzz/ - FTP
# listings, known_hosts, archive listings, Total Commander's INI files, the terminal's OSC
# sequences, plugin manifests, the certificate viewer's files, and (RC-110-68) an MCP request over
# stdio with its arguments, an ssh_config, the update manifest and the multi-rename rules - each
# for a while with libFuzzer, on the nightly pinned in scripts/lib/tool-versions.sh (fuzz/ is a
# workspace of its own; the main one stays on stable). .github/workflows/fuzz.yml runs this weekly
# on GitHub.
#
# Usage:
#   scripts/fuzz.sh                       # every target, 60 s each
#   scripts/fuzz.sh --seconds 300         # every target, 5 min each (the nightly run)
#   scripts/fuzz.sh tc_ini osc            # only these
#   scripts/fuzz.sh --list                # the targets, one per line (takes no lock)
#   scripts/fuzz.sh --build               # only build every target (or the ones named): the
#                                         # stage `fuzz-build` of scripts/check.sh --full, so a
#                                         # change that breaks a target is seen before the nightly
#                                         # run; the same build directory and flags as a run, so
#                                         # either warms the other
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
# Exit 0 when no target found anything (--build: every target built); 1 when one found something
# or did not build or run;
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
    echo "usage: scripts/fuzz.sh [<target>...] [--seconds N | --build] | --list" >&2
    exit 2
}

seconds=60
build_only=0
timed=0
targets=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seconds)
            [[ $# -ge 2 ]] || usage
            seconds="$2"
            timed=1
            shift 2
            ;;
        --build)
            build_only=1
            shift
            ;;
        -*) usage ;;
        *)
            targets+=("$1")
            shift
            ;;
    esac
done
[[ "$seconds" =~ ^[1-9][0-9]*$ ]] || usage
[[ "$build_only" -eq 0 || "$timed" -eq 0 ]] || usage
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
named="${#targets[@]}"
[[ "$named" -gt 0 ]] || targets=("${known[@]}")

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

# The files whose bytes start a target's corpus: the test data of its parser. The targets with
# none on disk have their seeds written instead (write_seeds).
seeds() {
    case "$1" in
        archive_listing) find crates/rc-vfs/tests/fixtures/archives -type f ! -name '*.py' ;;
        ftp_listing) find crates/rc-remote/tests/fixtures/ftp_list -type f ;;
        known_hosts) find crates/rc-remote/tests/fixtures/known_hosts -type f ;;
        plugin_manifest) find plugins -mindepth 2 -maxdepth 2 -name manifest.toml ;;
        tc_ini) find crates/rc-engine/tests/fixtures/total_commander -type f -iname '*.ini' ;;
        git_porcelain) find crates/rc-engine/tests/fixtures/git_porcelain -type f ;;
        cert_viewer) find crates/rc-cert/tests/fixtures -type f ;;
        ssh_config) find crates/rc-engine/tests/fixtures/ssh_config -type f ;;
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

# What an agent sends `rcommander mcp`, a JSON-RPC message per line (RC-110-68): the handshake and
# calls whose arguments fit, one with a misspelt key, one with a value of the wrong type.
# shellcheck disable=SC2016  # `$1` in a replacement is the regex group, not the shell's
mcp_seeds() {
    {
        echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fuzz","version":"1.0"}}}'
        echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'
        echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
        echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ops_move","arguments":{"sources":["a.txt"],"target":"/tmp","on_conflict":"skip","keep_attributes":true}}}'
    } > "$1/seed-mcp-session"
    echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ops_move","arguments":{"source":["a.txt"],"confirmation":"abc"}}}' > "$1/seed-mcp-typo"
    echo '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"multi_rename_preview","arguments":{"name_mask":"[N]_[C]","case":"lower","counter_width":3,"search":{"find":"a(.)","replace":"$1","regex":true},"panel":"left"}}}' > "$1/seed-mcp-rename"
    echo '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"files_set_attributes","arguments":{"paths":["x"],"hidden":"yes","apply_to":"both","modified":"2026-11-25T10:00:00Z"}}}' > "$1/seed-mcp-type"
}

# A manifest's payload as a release publishes it (RC-110-68); the target signs it, on either
# channel, and reads the bytes as an envelope as well.
update_seeds() {
    local artifact='"platform":"windows","arch":"x86_64","kind":"zip","url":"https://github.com/degoya/rCommander/releases/download/v1.8.0/rcommander-1.8.0-windows-x64.zip","sha256":"'
    artifact="$artifact$(printf 'ab%.0s' {1..32})\",\"size\":1024"
    printf '{"schema_version":1,"sequence":1788220800,"issued_at":"2026-09-01T00:00:00Z","not_after":"2027-02-01T00:00:00Z","channel":"stable","version":"1.8.0","released_at":"2026-09-01T00:00:00Z","notes":"- Update check","artifacts":[{%s}]}' \
        "$artifact" > "$1/seed-update-stable"
    printf '{"schema_version":1,"sequence":1788220801,"issued_at":"2026-09-01T00:00:01.250Z","not_after":"2027-02-01T00:00:00Z","channel":"beta","version":"1.9.0-beta.1","released_at":"2026-09-01T00:00:00Z","artifacts":[],"schema_change":false}' \
        > "$1/seed-update-beta"
    printf '{"payload":{"schema_version":1},"signatures":[{"key_id":"rcommander-update-v1","algorithm":"ed25519","signature":"AAAA"}]}' \
        > "$1/seed-update-envelope"
}

# A multi-rename rule (RC-110-68): four switch bytes - case and the search's flags, the file
# system with a search and a clipboard and the counter's width, the counter's start and step -
# then name mask, extension mask, search, replacement, clipboard and date format, one per line.
# shellcheck disable=SC2016  # `$2-$1` is the replacement's regex groups, not the shell's
rename_seeds() {
    printf '\000\000\001\001[N]\n[E]' > "$1/seed-rename-keep"
    printf '\001\061\012\005[N2-5]_[C]_[YMD]\n[E]' > "$1/seed-rename-counter"
    printf '\014\016\001\001[N] [X]\n[E2-]\n(\\w+) (\\d+)\n$2-$1\nclip\ndd.MM.yyyy' > "$1/seed-rename-regex"
    printf '\040\005\001\001[N1]\\[N]\n[E]\no\n0' > "$1/seed-rename-subfolder"
}

# Writes the seeds of a target without test data on disk into the folder $2; 1 for any other.
write_seeds() {
    case "$1" in
        osc) osc_seeds "$2" ;;
        everything_reply) everything_seeds "$2" ;;
        mcp_request) mcp_seeds "$2" ;;
        update_manifest) update_seeds "$2" ;;
        rename_pattern) rename_seeds "$2" ;;
        *) return 1 ;;
    esac
}

# Copies the seeds into `corpus` once; a seed already there (by its name) is left alone. Written
# seeds are written beside the corpus first, then copied the same way.
seed() {
    local target="$1" corpus="$2" file name count=0 written="$FUZZ_DIR/written-seeds/$1"
    rm -rf -- "$written" && mkdir -p "$written" || return 1
    if write_seeds "$target" "$written"; then
        for file in "$written"/seed-*; do
            [[ -e "$file" ]] || continue
            [[ -e "$corpus/${file##*/}" ]] || cp -- "$file" "$corpus/"
        done
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

# --build: the targets compiled and linked as a run would, nothing run. Every target in one cargo
# call unless some were named.
if [[ "$build_only" -eq 1 ]]; then
    echo "==> building ${targets[*]} on $TOOLCHAIN (jobs: $JOBS, log: $LOG/build.log)"
    build_targets=("")
    [[ "$named" -eq 0 ]] || build_targets=("${targets[@]}")
    failed=0
    for target in "${build_targets[@]}"; do
        # shellcheck disable=SC2086  # an empty target is no argument: cargo-fuzz then builds all
        CARGO_BUILD_JOBS="$JOBS" cargo "+$TOOLCHAIN" fuzz build --fuzz-dir "$ROOT/fuzz" \
            --target-dir "$FUZZ_DIR/build" --debug-assertions $target >> "$LOG/build.log" 2>&1 < /dev/null \
            || failed=1
    done
    if [[ "$failed" -eq 0 ]]; then
        echo "==> ${#targets[@]} fuzz targets built"
    else
        echo "!! a fuzz target did not build; the end of $LOG/build.log:"
        tail -n 40 "$LOG/build.log" | sed 's/^/   /'
    fi
    exit "$failed"
fi

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
