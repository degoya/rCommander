#!/usr/bin/env bash
#
# The release of rCommander, end to end, with evidence that cannot be faked into a green
# (RC-100-32, as rDownloader cuts its releases). Every step appends its raw output to an evidence
# log with this run's nonce, its exit code and its byte count; the tag is refused unless every
# step before it has a passing, non-empty record of this run (scripts/lib/release-evidence.sh).
# The steps and their order: scripts/lib/release-plan.sh.
#
# Usage:
#   scripts/release-pipeline.sh 0.9.0             # everything up to the tag and a local public
#                                                 # export; pushes nothing
#   scripts/release-pipeline.sh 0.9.0 --push      # ... with the public CI before the tag, and
#                                                 # pushes main, development, the tag, the public
#                                                 # export, the wiki and the website
#   scripts/release-pipeline.sh 0.9.0 --resume    # continue the run the log started; after a
#                                                 # change of the tree every step but preflight
#                                                 # runs again
#   scripts/release-pipeline.sh 0.9.0 --plan      # print the steps and exit
#   scripts/release-pipeline.sh 0.9.0-beta.1      # a pre-release: no merge into main, the tag on
#                                                 # development, the public export alone
#
# Started detached by scripts/release-start.sh. Every start and every --push only on the owner's
# word (.claude/skills/release). There is no --skip, --no-verify or --force: every flag this
# script does not have is a green it cannot report falsely.
#
# Takes the build lock (scripts/lib/lock.sh) for the whole run; the locked scripts it calls hold
# the same one. Never wrap it in flock.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Sourced before the `cd`, because the lock library resolves this script's own path from $0.
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
# shellcheck source=lib/release-tag.sh
source "$ROOT/scripts/lib/release-tag.sh"
# shellcheck source=lib/release-plan.sh
source "$ROOT/scripts/lib/release-plan.sh"
cd "$ROOT"

RELEASE_BRANCH="${RC_RELEASE_BRANCH:-development}"
MAIN_BRANCH="${RC_MAIN_BRANCH:-main}"
VERSION=""
DO_PUSH=0
RESUME=0
PLAN_ONLY=0
# The parse shifts every argument away; the lock re-executes this script with the original ones.
ORIGINAL_ARGS=("$@")
while [[ $# -gt 0 ]]; do
    case "$1" in
        --push) DO_PUSH=1 ;;
        --resume) RESUME=1 ;;
        --plan) PLAN_ONLY=1 ;;
        -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        -*) echo "unknown argument: $1" >&2; exit 2 ;;
        *) [[ -z "$VERSION" ]] || { echo "one version only" >&2; exit 2; }; VERSION="$1" ;;
    esac
    shift
done
if ! rc_release_version "$VERSION"; then
    echo "usage: scripts/release-pipeline.sh <X.Y.Z | X.Y.Z-beta.N> [--push] [--resume] [--plan]" >&2
    exit 2
fi
PRERELEASE=0
rc_is_prerelease "$VERSION" && PRERELEASE=1
LOG="$ROOT/artifacts/release-evidence-$VERSION.log"

if [[ "$PLAN_ONLY" -eq 1 ]]; then
    echo "release $VERSION$([[ $PRERELEASE -eq 1 ]] && echo ' (pre-release)'): ${#RC_RELEASE_STEPS[@]} steps, push $(
        [[ $DO_PUSH -eq 1 ]] && echo enabled || echo disabled)"
    for id in "${RC_RELEASE_STEPS[@]}"; do
        if why="$(rc_release_step_skipped "$id" "$DO_PUSH" "$PRERELEASE")"; then
            echo "  - $id ($why)"
        elif [[ "$id" == publish-public && "$PRERELEASE" -eq 1 ]]; then
            echo "  - $id (the public export only: no wiki, no website)"
        elif [[ "$id" == publish-public && "$DO_PUSH" -eq 0 ]]; then
            echo "  - $id (committed locally, not pushed)"
        else
            echo "  - $id"
        fi
    done
    echo "evidence: $LOG"
    exit 0
fi

# Sourcing this file with RELEASE_PIPELINE_LIB=1 defines the steps and the evidence without
# running a release or taking the lock, so the script tests can ask the gates what they refuse.
[[ -n "${RELEASE_PIPELINE_LIB:-}" ]] || rc_take_lock "${ORIGINAL_ARGS[@]}"

# shellcheck source=lib/release-evidence.sh
source "$ROOT/scripts/lib/release-evidence.sh"
# shellcheck source=lib/release-docs.sh
source "$ROOT/scripts/lib/release-docs.sh"
# shellcheck source=lib/release-steps-build.sh
source "$ROOT/scripts/lib/release-steps-build.sh"
# shellcheck source=lib/release-steps-publish.sh
source "$ROOT/scripts/lib/release-steps-publish.sh"

mkdir -p "$ROOT/artifacts"
if [[ "$RESUME" -eq 1 ]]; then
    [[ -f "$LOG" ]] || { echo "--resume, but $LOG does not exist" >&2; exit 1; }
    NONCE="$(rc_release_nonce "$LOG")"
    [[ -n "$NONCE" ]] || { echo "$LOG has no run header to resume" >&2; exit 1; }
    logged="$(rc_release_logged_version "$LOG")"
    [[ "$logged" == "$VERSION" ]] || { echo "$LOG is evidence for $logged, not $VERSION" >&2; exit 1; }
    printf '##RC-RESUME nonce=%s push=%s at=%s\n' "$NONCE" "$DO_PUSH" "$(date -Is)" >> "$LOG"
    echo "==> resuming run $NONCE from $LOG"
else
    NONCE="$(date +%s)-$$-$RANDOM"
    printf '##RC-RELEASE version=%s nonce=%s push=%s head=%s branch=%s started=%s host=%s\n' \
        "$VERSION" "$NONCE" "$DO_PUSH" "$(git rev-parse HEAD)" "$(git rev-parse --abbrev-ref HEAD)" \
        "$(date -Is)" "$(hostname)" > "$LOG"
fi

if [[ -n "${RELEASE_PIPELINE_LIB:-}" ]]; then
    # shellcheck disable=SC2317  # reached only when the file is run, not sourced
    return 0 2> /dev/null || exit 0
fi

[[ "$RESUME" -eq 0 ]] || resume_tree_check | tee -a "$LOG"

for id in "${RC_RELEASE_STEPS[@]}"; do
    if why="$(rc_release_step_skipped "$id" "$DO_PUSH" "$PRERELEASE")"; then
        echo
        echo "==> [$id] $why"
        [[ "$id" != push ]] || echo "    publish with: git push origin $(
            [[ $PRERELEASE -eq 0 ]] && echo "$MAIN_BRANCH ")$RELEASE_BRANCH v$VERSION"
        continue
    fi
    if [[ "$id" == merge-main ]]; then
        trap return_to_release_branch EXIT
        if [[ "$RESUME" -eq 1 ]] && step_is_green merge-main \
            && [[ "$(git rev-parse --abbrev-ref HEAD)" != "$MAIN_BRANCH" ]]; then
            echo "==> merge-main is green in this run; back onto $MAIN_BRANCH for the steps after it"
            git checkout -q "$MAIN_BRANCH"
        fi
    fi
    run_step "$id" "step_${id//-/_}"
done

echo
echo "==> $VERSION released$([[ $DO_PUSH -eq 1 ]] && echo " and pushed" || echo ", not pushed")"
echo "    evidence: $LOG"
