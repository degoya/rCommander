#!/usr/bin/env bash
#
# Runs the public GitHub CI on a tree of this repository before it is merged or tagged
# (RC-100-31, adapted from rDownloader's scripts/public-ci.sh): exports it to the public
# repository as a ci/ branch (scripts/export-public.sh --branch, its commit marked [skip ci]),
# starts ci.yml there by hand for the runner images no recorded green covers, waits, records the
# green per image and tree, and deletes the public branch. On red it prints the failed jobs' lines
# that say what broke (scripts/ci-log.sh) and the branch stays, so the run can be read next to its
# tree; the next export of the same name replaces it. A run of the same tree that another call on
# this machine started, still going or green, is waited for instead of exported again - the
# release chain's step beside a run started ahead of it (scripts/lib/public-ci.sh, the marker).
#
# A wave's integration branch runs this on Windows before it is merged into development (the
# `wave` skill); the release pipeline runs the candidate as ci/<version> (RC-100-32).
#
# Usage:
#   scripts/public-ci.sh integration/0.9-w1 --platforms windows   # the wave's gate: the branch,
#                                                                 # exported as ci/integration-0.9-w1
#   scripts/public-ci.sh ci/0.9.0                                 # HEAD as ci/0.9.0, every image
#   scripts/public-ci.sh ci/0.9.0 --ref <commit>                  # another commit under that name
#   scripts/public-ci.sh fix/x --platforms windows-2025,macos-15  # runner images by name
#
# --platforms takes linux (ubuntu-24.04, ubuntu-24.04-arm), windows (windows-2025), macos
# (macos-15, macos-15-intel) or runner images, comma-separated; without it, all of them. An image
# already green for the tree is not run again; with every one green nothing is exported.
#
# Exit: 0 green, 1 red or not finished (the branch is kept; also `gh auth status` failing on a
# timeout or the network), 2 a wrong call, no gh or gh not signed in.
#
# Outward: the tree becomes public while the run lasts, minus what scripts/public-exclude.txt
# leaves out - every export is a push, so it runs only on the owner's word (RC-100-31).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/public-ci.sh
source "$ROOT/scripts/lib/public-ci.sh"
# shellcheck source=lib/workspace-version.sh
source "$ROOT/scripts/lib/workspace-version.sh"
cd "$ROOT"

usage() {
    echo "usage: scripts/public-ci.sh <branch|ci/<name>> [--platforms linux,windows,macos] [--ref <commit>]" >&2
    exit 2
}

target=""
platforms=""
ref=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --platforms) [[ $# -ge 2 ]] || usage; platforms="$2"; shift 2 ;;
        --ref) [[ $# -ge 2 ]] || usage; ref="$2"; shift 2 ;;
        -h | --help) sed -n '2,30p' "$0"; exit 0 ;;
        -*) echo "unknown argument: $1" >&2; usage ;;
        *) [[ -z "$target" ]] || usage; target="$1"; shift ;;
    esac
done
[[ -n "$target" ]] || usage

# A ci/ name is the public branch itself, of HEAD unless --ref names another commit; any other
# name is a local branch or commit, exported as ci/ and the name with its slashes flattened.
if [[ "$target" == ci/?* ]]; then
    public_branch="$target"
    ref="${ref:-HEAD}"
else
    public_branch="ci/${target//\//-}"
    ref="${ref:-$target}"
fi
git check-ref-format --branch "$public_branch" > /dev/null 2>&1 \
    || { echo "not a branch name: $public_branch" >&2; exit 2; }
commit="$(git rev-parse --verify --quiet "$ref^{commit}")" \
    || { echo "no such branch or commit: $ref" >&2; exit 2; }
tree="$(git rev-parse "$commit^{tree}")"

image_list="$(rc_public_ci_images "${platforms:-$RC_PUBLIC_CI_ALL}")" || exit 2
images=()
while IFS= read -r image; do
    [[ -n "$image" ]] || continue
    case " ${images[*]} " in *" $image "*) ;; *) images+=("$image") ;; esac
done <<< "$image_list"

# export-public.sh names the version in its commit message; it takes X.Y.Z or X.Y.Z-beta.N.
version="$(git show "$commit:Cargo.toml" | rc_workspace_version)"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$ ]] || version="${version%%[-+]*}"
[[ -n "$version" ]] || { echo "no workspace version in $ref:Cargo.toml" >&2; exit 2; }

echo "==> $target (${commit:0:12}, tree ${tree:0:12}) on GitHub as $public_branch"
missing=()
for image in "${images[@]}"; do
    if rc_ci_green "$ROOT" "$tree" "$image"; then
        echo "  $image: green for this tree already"
    else
        echo "  $image: to run"
        missing+=("$image")
    fi
done
if [[ ${#missing[@]} -eq 0 ]]; then
    echo "==> $target is green on GitHub (${images[*]}) already; nothing was run"
    exit 0
fi

rc_public_ci_gh_ready || exit $?
# A run of this tree that another call started from this machine and that is still going (or
# green) is waited for, not exported again: the re-push would cancel it.
if sha="$(rc_public_ci_running "$public_branch" "$tree" "${missing[@]}")"; then
    echo "==> a run of this tree is already on $public_branch (${sha:0:12}); waiting for it instead of exporting again"
else
    "$ROOT/scripts/export-public.sh" "$version" --ref "$commit" --branch "$public_branch" --skip-push-ci \
        || { echo "!! the export of $public_branch failed; nothing was run" >&2; exit 1; }
    sha="$(git -C "$PUBLIC_DIR" rev-parse --verify --quiet "refs/heads/$public_branch")" \
        || { echo "!! the export left no $public_branch in $PUBLIC_DIR" >&2; exit 1; }
    rc_public_ci_dispatch "$public_branch" "$(rc_public_ci_json "${missing[@]}")" \
        || { echo "!! ci.yml could not be started on $public_branch" >&2; exit 1; }
    rc_public_ci_mark "$public_branch" "$sha" "$tree" "${missing[@]}" \
        || echo "!! no marker for the run on $public_branch; a second call would export again" >&2
fi
if ! rc_public_ci_wait "$public_branch" "$sha"; then
    # What broke, from the failed jobs' logs; the run itself goes on on GitHub.
    if [[ -n "$RC_PUBLIC_CI_RED_RUN" ]]; then
        "$ROOT/scripts/ci-log.sh" "$RC_PUBLIC_CI_RED_RUN" --repo "$PUBLIC_REPO" \
            || echo "!! the failed jobs' logs could not be read: scripts/ci-log.sh $RC_PUBLIC_CI_RED_RUN" >&2
    fi
    echo "!! $target is not green on GitHub; it is not merged or tagged until it is." >&2
    exit 1
fi
rc_record_ci "$ROOT" "$tree" "${missing[@]}"
rc_public_ci_delete "$public_branch" || echo "!! could not delete $public_branch; delete it by hand" >&2
echo "==> $target is green on GitHub (${images[*]}); recorded for ${missing[*]}"
