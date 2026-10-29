# shellcheck shell=bash
#
# The public CI run of scripts/public-ci.sh (RC-100-31, adapted from rDownloader's
# scripts/lib/public-ci.sh and lib/verified.sh, without their adoption of earlier runs, the
# release workflows and the documentation-only tree comparison): export a tree to the public
# repository as a ci/ branch, let GitHub's runners check it on the systems this machine cannot,
# wait, and delete the branch on green. A red run keeps its branch, so the failure can be read
# next to its tree (scripts/ci-log.sh).
#
# Sourced, never run. Every function returns non-zero on a failure instead of exiting, so a
# caller without errexit (the release pipeline's step) reads the status itself.
#
# A green run is recorded per runner image and tree in rc-verified-ci of the repository's common
# git directory - one file for every worktree - and a tree green on an image is not run there
# again. A red run records nothing.
#
# Environment:
#   RC_PUBLIC_DIR         the local clone of the public repository (~/projects/rCommander-public)
#   RC_PUBLIC_REPO        the GitHub repository (degoya/rCommander)
#   RC_PUBLIC_CI_TIMEOUT  seconds to wait for a run to appear at all (900)
#   RC_PUBLIC_CI_CEILING  seconds after which a run still going is given up (21600, the six hours
#                         a hosted job may run; every job of ci.yml has its own timeout)
#   RC_PUBLIC_CI_POLL     seconds between two looks (60)
#   RC_PUBLIC_CI_CLOCK    tests only: a file whose number is the clock both limits are read on, in
#                         place of bash's SECONDS; the tests' stand-in gh moves it

PUBLIC_DIR="${RC_PUBLIC_DIR:-$HOME/projects/rCommander-public}"
PUBLIC_REPO="${RC_PUBLIC_REPO:-degoya/rCommander}"
PUBLIC_CI_TIMEOUT="${RC_PUBLIC_CI_TIMEOUT:-900}"
PUBLIC_CI_CEILING="${RC_PUBLIC_CI_CEILING:-21600}"
PUBLIC_CI_POLL="${RC_PUBLIC_CI_POLL:-60}"
# Every system ci.yml checks; read by the script that sources this file.
# shellcheck disable=SC2034
RC_PUBLIC_CI_ALL="linux,windows,macos"
# One run as a line, "<id> <status> <conclusion or -> <url> <name>", from `gh run list`'s JSON.
RC_PUBLIC_CI_RUN_JQ='.[] | "\(.databaseId) \(.status) \(if (.conclusion // "") == "" then "-" else .conclusion end) \(.url) \(.name)"'

rc_public_ci_gh_ready() {
    command -v gh > /dev/null || { echo "gh is required to watch the public CI" >&2; return 1; }
    gh auth status --hostname github.com > /dev/null 2>&1 \
        || { echo "gh is not signed in to github.com (gh auth login)" >&2; return 1; }
}

# The runner images, one per line, of a comma-separated list of systems (linux, windows, macos)
# or runner images: the images of ci.yml's `platforms` default, a system being all of its images.
rc_public_ci_images() {
    local item
    local -a items
    IFS=',' read -r -a items <<< "$1"
    [[ ${#items[@]} -gt 0 ]] || { echo "no platform named" >&2; return 2; }
    for item in "${items[@]}"; do
        case "$item" in
            linux) printf '%s\n' ubuntu-24.04 ubuntu-24.04-arm ;;
            windows) printf '%s\n' windows-2025 ;;
            macos) printf '%s\n' macos-15 macos-15-intel ;;
            ubuntu-* | windows-* | macos-*) printf '%s\n' "$item" ;;
            *) echo "unknown platform: $item (linux, windows, macos or a runner image)" >&2; return 2 ;;
        esac
    done
}

# The JSON list ci.yml's `platforms` input takes, of the runner images $@.
rc_public_ci_json() {
    local image out=""
    for image in "$@"; do out+="${out:+,}\"$image\""; done
    printf '[%s]\n' "$out"
}

# The file the greens are recorded in, for checkout $1.
rc_verified_ci_file() {
    printf '%s/rc-verified-ci\n' "$(git -C "$1" rev-parse --path-format=absolute --git-common-dir)"
}

# Whether runner image $3 has a recorded green for tree $2 of checkout $1.
rc_ci_green() {
    local file
    file="$(rc_verified_ci_file "$1")"
    [[ -f "$file" ]] && awk -v tree="$2" -v image="$3" '$1 == tree && $2 == image { found = 1 } END { exit !found }' "$file"
}

# Records a green of tree $2 of checkout $1 on the runner images $3...: "<tree> <image> <time>".
rc_record_ci() {
    local root="$1" tree="$2" file image when
    shift 2
    file="$(rc_verified_ci_file "$root")" || return 1
    when="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for image in "$@"; do printf '%s %s %s\n' "$tree" "$image" "$when" >> "$file"; done
}

# Starts ci.yml on public branch $1 for the platforms in JSON list $2.
rc_public_ci_dispatch() {
    echo "starting ci.yml on $1 for $2"
    gh workflow run ci.yml --repo "$PUBLIC_REPO" --ref "$1" -f platforms="$2"
}

# The clock the limits are read on: bash's SECONDS, or the number in RC_PUBLIC_CI_CLOCK.
rc_public_ci_now() {
    if [[ -n "${RC_PUBLIC_CI_CLOCK:-}" ]]; then cat "$RC_PUBLIC_CI_CLOCK"; else echo "$SECONDS"; fi
}

# The first failed job of run $1 as "<name>: <conclusion> (<url>)", or nothing: a job red long
# before its run ends ends the wait at once.
rc_public_ci_failed_job() {
    gh run view "$1" --repo "$PUBLIC_REPO" --json jobs \
        --jq '.jobs[] | select(.conclusion == "failure" or .conclusion == "timed_out" or .conclusion == "cancelled") | "\(.name): \(.conclusion) (\(.url))"' \
        2> /dev/null < /dev/null | head -n 1 || true
}

# Waits for the dispatched runs on public branch $1 at commit $2 to finish. Returns 0 when every
# one succeeded, 1 at once when a job or a run failed - its run's id then in RC_PUBLIC_CI_RED_RUN
# for scripts/ci-log.sh - when no run appeared within RC_PUBLIC_CI_TIMEOUT, or when one was still
# going at RC_PUBLIC_CI_CEILING. Prints what changed.
rc_public_ci_wait() {
    local branch="$1" sha="$2" now deadline ceiling listed last="" seen=0 red id status line
    RC_PUBLIC_CI_RED_RUN=""
    echo "waiting for the CI of $PUBLIC_REPO on $branch at ${sha:0:12} (for as long as it runs)"
    now="$(rc_public_ci_now)"
    deadline=$((now + PUBLIC_CI_TIMEOUT))
    ceiling=$((now + PUBLIC_CI_CEILING))
    while :; do
        listed="$(gh run list --repo "$PUBLIC_REPO" --branch "$branch" --commit "$sha" \
            --event workflow_dispatch --json databaseId,status,conclusion,url,name \
            --jq "$RC_PUBLIC_CI_RUN_JQ" 2> /dev/null < /dev/null)" || listed=""
        if [[ -n "$listed" ]]; then
            seen=1
            if [[ "$listed" != "$last" ]]; then
                while IFS= read -r line; do echo "  $(date +%H:%M:%S) run $line"; done <<< "$listed"
                last="$listed"
            fi
            red="$(awk '$2 == "completed" && $3 !~ /^(success|skipped|neutral)$/ { print "run " $1 ": " $3 " (" $4 ")"; exit }' <<< "$listed")"
            [[ -z "$red" ]] || RC_PUBLIC_CI_RED_RUN="$(awk '{ print $1 }' <<< "${red#run }")"
            RC_PUBLIC_CI_RED_RUN="${RC_PUBLIC_CI_RED_RUN%:}"
            while [[ -z "$red" ]] && read -r id status _; do
                [[ "$status" != completed ]] || continue
                red="$(rc_public_ci_failed_job "$id")"
                [[ -z "$red" ]] || RC_PUBLIC_CI_RED_RUN="$id"
            done <<< "$listed"
            if [[ -n "$red" ]]; then
                echo "the public CI is red: $red; $branch is kept for inspection" >&2
                return 1
            fi
            if ! awk '$2 != "completed" { found = 1 } END { exit !found }' <<< "$listed"; then
                echo "the public CI is green on ${sha:0:12}"
                return 0
            fi
        fi
        now="$(rc_public_ci_now)"
        if [[ "$seen" -eq 0 ]] && ((now >= deadline)); then
            echo "no CI run appeared within ${PUBLIC_CI_TIMEOUT}s; $branch is kept" >&2
            return 1
        fi
        if ((now >= ceiling)); then
            echo "the public CI did not finish within ${PUBLIC_CI_CEILING}s; $branch is kept" >&2
            return 1
        fi
        sleep "$PUBLIC_CI_POLL"
    done
}

# Deletes branch $1 from the public repository and from the local clone.
rc_public_ci_delete() {
    git -C "$PUBLIC_DIR" push --quiet origin --delete "$1" || return 1
    # Before the first release the clone has no main to return to and still stands on the
    # branch; its local ref then stays until the next export.
    git -C "$PUBLIC_DIR" branch -D "$1" > /dev/null 2>&1 || true
    echo "$1 deleted"
}
