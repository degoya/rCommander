#!/usr/bin/env bash
#
# scripts/fuzz.sh in a throwaway repository, without a nightly: the script, its libraries and
# fuzz/Cargo.toml are copied beside a little test data, and stand-ins for cargo and rustup on
# PATH record what they are asked. --list names the six targets of fuzz/Cargo.toml; wrong
# arguments and an unknown target are refused; a machine without the nightly or cargo-fuzz runs
# nothing and says what to install; a run fuzzes every target (or the ones named) for the
# seconds asked, in the build lock's jobs, from a corpus seeded with the test data, after copying
# the main Cargo.lock in; a finding and a target that does not run fail the run and are named,
# while the other targets still run.
#
# Pure bash: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/fuzz.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
REPO="$SCRATCH/repo"
CALLS="$SCRATCH/calls"
FUZZ="$SCRATCH/fuzz-dir"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=../lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
has() { grep -qF -- "$2" "$1"; }

mkdir -p "$REPO/scripts/lib" "$REPO/fuzz" "$REPO/crates/rc-remote/tests/fixtures/ftp_list" \
    "$REPO/plugins/a-viewer" "$SCRATCH/bin"
cp "$ROOT/scripts/fuzz.sh" "$REPO/scripts/"
cp "$ROOT/scripts/lib/tool-versions.sh" "$ROOT/scripts/lib/jobs.sh" "$ROOT/scripts/lib/lock.sh" \
    "$REPO/scripts/lib/"
cp "$ROOT/fuzz/Cargo.toml" "$REPO/fuzz/"
echo "# the main lock" > "$REPO/Cargo.lock"
echo "drwxr-xr-x 2 ftp ftp 4096 Jan 10 12:00 pub" > "$REPO/crates/rc-remote/tests/fixtures/ftp_list/unix.txt"
echo "manifest_version = 1" > "$REPO/plugins/a-viewer/manifest.toml"

# cargo: records its arguments and CARGO_BUILD_JOBS; `fuzz --version` answers FAKE_FUZZ_VERSION
# (fails with FAKE_NO_FUZZ); `fuzz run` leaves a finding for FAKE_FIND, breaks for FAKE_BREAK.
cat > "$SCRATCH/bin/cargo" << EOF
#!/usr/bin/env bash
echo "cargo \$* jobs=\${CARGO_BUILD_JOBS:-}" >> "$CALLS"
[[ "\$2 \$3" == "fuzz --version" ]] && { [[ -z "\${FAKE_NO_FUZZ:-}" ]] || exit 101; echo "cargo-fuzz \${FAKE_FUZZ_VERSION:-$RC_CARGO_FUZZ_VERSION}"; exit 0; }
args=("\$@")
for ((i = 0; i < \${#args[@]}; i++)); do
    [[ "\${args[i]}" == --fuzz-dir ]] && dir="\${args[i + 1]}"
    [[ "\${args[i]}" == --debug-assertions ]] && target="\${args[i + 1]}"
done
if [[ "\$target" == "\${FAKE_FIND:-}" ]]; then
    mkdir -p "\$dir/artifacts/\$target" && touch "\$dir/artifacts/\$target/crash-1"
    exit 77
fi
[[ "\$target" == "\${FAKE_BREAK:-}" ]] && exit 101
echo "stat::number_of_executed_units: 1234"
EOF
cat > "$SCRATCH/bin/rustup" << EOF
#!/usr/bin/env bash
printf '%s\n' "\${FAKE_TOOLCHAINS-$RC_FUZZ_NIGHTLY-x86_64-unknown-linux-gnu}" "$(rc_rust_version "$ROOT")-x86_64-unknown-linux-gnu"
EOF
chmod +x "$SCRATCH/bin/cargo" "$SCRATCH/bin/rustup"
export PATH="$SCRATCH/bin:$PATH" RC_FUZZ_DIR="$FUZZ" RC_FUZZ_LOGS="$SCRATCH/logs" \
    RC_LOCK_FILE="$SCRATCH/build.lock" RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1 RC_JOBS=3
unset RC_LOCK_HELD RC_FUZZ_TOOLCHAIN RC_FUZZ_TIMEOUT
FUZZ_SH="$REPO/scripts/fuzz.sh"

run_status "$FUZZ_SH" --list
expect_status "--list passes" 0
expect "and names the six targets in fuzz/Cargo.toml's order" \
    "archive_listing ftp_listing known_hosts osc plugin_manifest tc_ini" "$(tr '\n' ' ' <<< "$output" | sed 's/ $//')"
expect_true "without asking cargo anything" '[[ ! -e "$CALLS" ]]'

run_status "$FUZZ_SH" --seconds 0
expect_status "--seconds 0 is refused" 2
run_status "$FUZZ_SH" --seconds
expect_status "--seconds without a number is refused" 2
run_status "$FUZZ_SH" --hours 1
expect_status "an unknown option is refused" 2
run_status "$FUZZ_SH" zip
expect_status "an unknown target is refused" 2
expect_output "and the targets are named" "the targets are: archive_listing ftp_listing"
RC_FUZZ_TIMEOUT=long run_status "$FUZZ_SH"
expect_status "RC_FUZZ_TIMEOUT that is no number is refused" 2

FAKE_TOOLCHAINS="" run_status "$FUZZ_SH"
expect_status "without the nightly nothing runs" 3
expect_output "and it says what to install" "rustup toolchain install $RC_FUZZ_NIGHTLY"
FAKE_NO_FUZZ=1 run_status "$FUZZ_SH"
expect_status "without cargo-fuzz nothing runs" 3
expect_output "and it says what to install" "cargo install --locked cargo-fuzz --version $RC_CARGO_FUZZ_VERSION"
expect_true "no target was run" '[[ ! -e "$CALLS" ]] || ! has "$CALLS" "fuzz run"'

rm -f "$CALLS"
run_status "$FUZZ_SH"
expect_status "a run without a finding passes" 0
expect_true "every target is fuzzed 60 s on the pinned nightly, with debug assertions, in RC_JOBS jobs" \
    '[[ "$(grep -c "fuzz run" "$CALLS")" -eq 6 ]] && has "$CALLS" "cargo +$RC_FUZZ_NIGHTLY fuzz run --fuzz-dir $REPO/fuzz --target-dir $FUZZ/build --debug-assertions tc_ini $FUZZ/corpus/tc_ini -- -max_total_time=60 -timeout=25 -print_final_stats=1 jobs=3"'
expect_output "each with its inputs" "ok      ftp_listing: 1234 inputs in 60 s"
expect_true "the corpus is seeded with the test data" '[[ -e "$FUZZ/corpus/ftp_listing/seed-crates_rc-remote_tests_fixtures_ftp_list_unix.txt" && -e "$FUZZ/corpus/plugin_manifest/seed-plugins_a-viewer_manifest.toml" ]]'
printf '\033]9;9;"C:\134Users\134me"\033\134' > "$SCRATCH/osc9"
expect_true "the OSC target with sequences of its own" 'cmp -s "$FUZZ/corpus/osc/seed-osc9" "$SCRATCH/osc9"'
expect_output "a target without test data says so" "note: no test data found to seed known_hosts"
expect_true "the main Cargo.lock was copied in" 'cmp -s "$REPO/Cargo.lock" "$REPO/fuzz/Cargo.lock"'
expect_true "the summary is kept" '[[ "$(cat "$SCRATCH"/logs/*/summary | wc -l)" -eq 6 ]]'

rm -f "$CALLS"
RC_FUZZ_TIMEOUT=9 run_status "$FUZZ_SH" --seconds 5 osc
expect_status "one target named passes" 0
expect_true "and only it is fuzzed, as long and with the timeout asked" \
    '[[ "$(grep -c "fuzz run" "$CALLS")" -eq 1 ]] && has "$CALLS" "osc $FUZZ/corpus/osc -- -max_total_time=5 -timeout=9 "'

rm -f "$CALLS"
RC_FUZZ_TOOLCHAIN=nightly-2030-01-01 FAKE_TOOLCHAINS=nightly-2030-01-01-x86_64-unknown-linux-gnu \
    run_status "$FUZZ_SH" osc
expect_true "RC_FUZZ_TOOLCHAIN picks another nightly" '[[ $status -eq 0 ]] && has "$CALLS" "cargo +nightly-2030-01-01 fuzz run"'

FAKE_FUZZ_VERSION=0.1.0 run_status "$FUZZ_SH" osc
expect_output "another cargo-fuzz than the pinned one is named" "note: cargo-fuzz 0.1.0 is installed"

rm -f "$CALLS"
FAKE_FIND=known_hosts FAKE_BREAK=osc run_status "$FUZZ_SH"
expect_status "a finding or a broken target fails the run" 1
expect_output "the finding is named" "FINDING known_hosts: $REPO/fuzz/artifacts/known_hosts/crash-1"
expect_output "so is the broken target" "FAILED  osc: exit 101 without a finding"
expect_true "and the targets after them still ran" 'has "$CALLS" "--debug-assertions tc_ini"'

finish_tests fuzz
