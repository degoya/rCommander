#!/usr/bin/env bash
#
# Starts scripts/release-pipeline.sh detached, so the chain survives the terminal, the editor and
# the agent that started it (RC-100-32, as rDownloader). The chain writes its own PID to a file,
# so waiting on it is `kill -0` on a number, never a pattern.
#
# Before anything detaches it refuses, in seconds and compiling nothing, a tree the chain would
# stop on later, each finding named with what fixes it: not on development, a dirty tree, the tag
# taken, the documents (scripts/lib/release-docs.sh, the docs-gate's own check, before the bump).
# A --resume continues a stopped run as it stands; its tree is the bumped one, so only the
# version is checked.
#
# Nothing here takes the build lock: the pipeline takes it. Never wrap this in flock.
#
# Usage:
#   scripts/release-start.sh 0.9.0              # start; prints where the log and the PID are
#   scripts/release-start.sh 0.9.0 --push       # ... and hand --push to the pipeline
#   scripts/release-start.sh 0.9.0 --resume     # continue the stopped run, detached
#   scripts/release-start.sh 0.9.0-beta.1       # a pre-release
#
# Files, under /tmp/claude-<uid>/release-<version>/ (RC_RELEASE_RUN_DIR overrides it):
#   chain.log   everything the chain prints; the evidence log is the pipeline's, in artifacts/
#   pid         the PID of the detached chain, written by the chain itself
#   exit        the chain's exit code, written when it ends; absent while it runs
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The pinned tools a user installs (docs/development.md: shellcheck through uv) sit in
# ~/.local/bin, which a shell started without the profile lacks; the chain's check then finds an
# older system shellcheck and fails `shellcheck-version` (0.10.0, 2026-10-30).
case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
# shellcheck source=lib/release-tag.sh
source "$ROOT/scripts/lib/release-tag.sh"
# shellcheck source=lib/release-docs.sh
source "$ROOT/scripts/lib/release-docs.sh"

chain=0
version=""
flags=()
for argument in "$@"; do
    case "$argument" in
        --chain) chain=1 ;;
        --push | --resume) flags+=("$argument") ;;
        -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        -*) echo "unknown argument: $argument" >&2; exit 2 ;;
        *) version="$argument" ;;
    esac
done
if ! rc_release_version "$version"; then
    echo "usage: scripts/release-start.sh <X.Y.Z | X.Y.Z-beta.N> [--push] [--resume]" >&2
    exit 2
fi

run_dir="${RC_RELEASE_RUN_DIR:-/tmp/claude-$(id -u)/release-$version}"
log="$run_dir/chain.log"
pid_file="$run_dir/pid"
exit_file="$run_dir/exit"

# --- the chain itself, detached ------------------------------------------------------------------
if [[ "$chain" -eq 1 ]]; then
    printf '%s\n' "$$" > "$pid_file"
    cd "$ROOT"
    echo "==> [release-start] scripts/release-pipeline.sh $version ${flags[*]} ($(date -Is))"
    status=0
    scripts/release-pipeline.sh "$version" "${flags[@]}" || status=$?
    echo
    echo "==> [release-start] chain ended with exit $status ($(date -Is))"
    printf '%s\n' "$status" > "$exit_file"
    exit "$status"
fi

# --- the preconditions ---------------------------------------------------------------------------
if [[ " ${flags[*]} " != *" --resume "* ]]; then
    refused=0
    cd "$ROOT"
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [[ "$branch" != "${RC_RELEASE_BRANCH:-development}" ]]; then
        echo "on '$branch'; a release is cut from '${RC_RELEASE_BRANCH:-development}' (git switch it)" >&2
        refused=1
    fi
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "the working tree is not clean; commit what belongs to the release (git status):" >&2
        git status --short >&2
        refused=1
    fi
    if git rev-parse -q --verify "refs/tags/v$version" > /dev/null; then
        echo "tag v$version already exists; a release takes a new version" >&2
        refused=1
    fi
    rc_release_docs_check "$version" --before-bump >&2 || refused=1
    if [[ "$refused" -ne 0 ]]; then
        echo "release $version not started; fix the above, then run scripts/release-start.sh $version again" >&2
        exit 1
    fi
fi

# --- the launcher --------------------------------------------------------------------------------
mkdir -p "$run_dir"
if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2> /dev/null && [[ ! -f "$exit_file" ]]; then
    echo "a release chain for $version is already running as PID $(cat "$pid_file")" >&2
    echo "  log: $log" >&2
    exit 1
fi
rm -f "$pid_file" "$exit_file"

setsid nohup "$0" --chain "$version" "${flags[@]}" > "$log" 2>&1 < /dev/null &
disown || true

# The chain writes its own PID; setsid may fork, so `$!` is not guaranteed to be it.
for _ in $(seq 1 50); do
    [[ -s "$pid_file" ]] && break
    sleep 0.1
done
if [[ ! -s "$pid_file" ]]; then
    echo "the chain did not start; see $log" >&2
    exit 1
fi
pid="$(cat "$pid_file")"
cat << INFO
==> release chain for $version started, detached, as PID $pid${flags[*]:+ (${flags[*]})}
    log:   $log
    pid:   $pid_file
    exit:  $exit_file (written when the chain ends)

    follow: tail -f $log
    wait:   while kill -0 $pid 2> /dev/null; do sleep 30; done; cat $exit_file
INFO
