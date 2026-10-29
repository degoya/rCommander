#!/usr/bin/env bash
#
# Tags the release `vX.Y.Z` at HEAD, annotated with the version's CHANGELOG.md section, so `git
# show vX.Y.Z` says what shipped (RC-100-32). Never pushes; the pipeline's `push` step does, on
# the owner's word.
#
# Refused unless every step before the tag in the last run of scripts/release-pipeline.sh for
# this version has a passing, non-empty record in its evidence log
# (artifacts/release-evidence-X.Y.Z.log, scripts/lib/release-evidence.sh), and HEAD's tree is the
# one the last of them recorded: a tag never goes on content the chain did not check. So it runs
# as the pipeline's `tag` step; there is no way around the evidence.
#
# Usage:
#   scripts/tag-release.sh 0.9.0
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/release-tag.sh
source "$ROOT/scripts/lib/release-tag.sh"
# shellcheck source=lib/release-plan.sh
source "$ROOT/scripts/lib/release-plan.sh"
# shellcheck source=lib/release-evidence.sh
source "$ROOT/scripts/lib/release-evidence.sh"
cd "$ROOT"

if [[ $# -ne 1 ]] || ! rc_release_version "$1"; then
    echo "usage: scripts/tag-release.sh <X.Y.Z | X.Y.Z-beta.N>" >&2
    exit 2
fi
VERSION="$1"
tag="v$VERSION"
current="$(scripts/set-version.sh)"
if [[ "$current" != "$VERSION" ]]; then
    echo "asked to tag $VERSION but Cargo.toml reports $current" >&2
    exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
    echo "the working tree has uncommitted changes; a tag must point at a finished commit" >&2
    git status --short >&2
    exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" > /dev/null; then
    echo "$tag already exists (pointing at $(git rev-parse --short "$tag"))" >&2
    exit 1
fi

LOG="$ROOT/artifacts/release-evidence-$VERSION.log"
NONCE="$(rc_release_nonce "$LOG")"
if [[ -z "$NONCE" ]]; then
    echo "no evidence: $LOG has no run of scripts/release-pipeline.sh $VERSION" >&2
    exit 1
fi
prerelease=0
rc_is_prerelease "$VERSION" && prerelease=1
mapfile -t required < <(rc_release_required_before tag "$(rc_release_push "$LOG" "$NONCE")" "$prerelease")
echo "the evidence of run $NONCE:"
if ! rc_release_gate "${required[@]}"; then
    echo "refusing to tag $tag: not every step before it passed in this run" >&2
    exit 1
fi
checked="$(marker_field "$(last_marker "${required[-1]}")" tree)"
if [[ "$checked" != "$(git rev-parse 'HEAD^{tree}')" ]]; then
    echo "refusing to tag $tag: HEAD's tree is not the one [${required[-1]}] recorded (${checked:0:12})" >&2
    exit 1
fi

message="$(awk -v heading="## [$VERSION] - " '
    /^## \[/ { inside = (index($0, heading) == 1) }
    inside { print }
' CHANGELOG.md)"
if [[ -z "$message" ]]; then
    echo "CHANGELOG.md has no '## [$VERSION] - <date>' section" >&2
    exit 1
fi

# verbatim: the default cleanup would strip the `## [X.Y.Z]` heading as a comment line.
git tag -a --cleanup=verbatim "$tag" -m "$message"
echo "==> tagged $tag at $(git rev-parse --short HEAD)"
echo "    not pushed; the pipeline's push step publishes it (git push origin $tag)"
