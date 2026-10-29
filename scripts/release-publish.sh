#!/usr/bin/env bash
#
# The signed update manifest of a release (RC-100-32 with RC-100-04), as the public repository's
# release.yml calls it after `scripts/release-assets.sh sums <dir>`:
#
#   scripts/release-publish.sh update-manifest <dir> <tag>
#
# Builds `<dir>/rcommander-update-stable.json` for a tag `vX.Y.Z`, `rcommander-update-beta.json`
# for `vX.Y.Z-beta.N`, over the assets in <dir>, with `rc-pack update manifest build` and checks it
# with `rc-pack update manifest verify`. The download base is the tag's GitHub release
# (GITHUB_REPOSITORY, default degoya/rCommander).
#
# Environment:
#   RCOMMANDER_UPDATE_SIGNING_KEY  the update key: its content (the repository secret) or the
#                                  path of its file. Unset or empty: a warning, no manifest, exit 0
#                                  - the release goes out and installations find no update.
#   RC_PACK                        the packager (default `rc-pack` on PATH)
#
# The key never reaches a log or the command line: its content goes into a file only this user can
# read, removed on exit.
set -euo pipefail

# shellcheck source=lib/release-tag.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/release-tag.sh"

usage() {
    echo "usage: scripts/release-publish.sh update-manifest <dir> <tag>" >&2
    exit 2
}

update_manifest() {
    local dir="$1" tag="$2" version channel key manifest pack="${RC_PACK:-rc-pack}"
    local secret="${RCOMMANDER_UPDATE_SIGNING_KEY:-}"
    version="${tag#v}"
    if [[ "$tag" != v* ]] || ! rc_release_version "$version"; then
        echo "release-publish: '$tag' is not a release tag (vX.Y.Z or vX.Y.Z-beta.N)" >&2
        exit 2
    fi
    [[ -d "$dir" ]] || { echo "release-publish: $dir is not a directory" >&2; exit 2; }
    if [[ -z "$secret" ]]; then
        echo "::warning::RCOMMANDER_UPDATE_SIGNING_KEY is not set; this release publishes no update manifest"
        exit 0
    fi
    command -v "$pack" > /dev/null || { echo "release-publish: the packager '$pack' is not there" >&2; exit 1; }
    channel=stable
    rc_is_prerelease "$version" && channel=beta

    if [[ -f "$secret" ]]; then
        key="$secret"
    else
        key="$(umask 077 && mktemp)"
        # shellcheck disable=SC2064  # the path is fixed now, on purpose
        trap "rm -f '$key'" EXIT
        printf '%s\n' "$secret" > "$key"
    fi
    manifest="$dir/rcommander-update-$channel.json"
    "$pack" update manifest build --channel "$channel" --version "$version" --assets "$dir" \
        --base-url "https://github.com/${GITHUB_REPOSITORY:-degoya/rCommander}/releases/download/$tag/" \
        --key "$key" --out "$manifest"
    "$pack" update manifest verify "$manifest" --assets "$dir"
    echo "release-publish: $manifest for $version ($channel), verified"
}

case "${1:-}" in
    update-manifest)
        [[ $# -eq 3 ]] || usage
        update_manifest "$2" "$3"
        ;;
    *) usage ;;
esac
