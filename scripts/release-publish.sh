#!/usr/bin/env bash
#
# The signed update manifest of a release (RC-100-32 with RC-100-04, RC-110-32), as the public
# repository's release.yml calls it:
#
#   scripts/release-publish.sh require-key                  # first, before anything is built
#   scripts/release-publish.sh update-manifest <dir> <tag>  # after `release-assets.sh sums <dir>`
#   scripts/release-publish.sh release-text <tag> <file>    # the GitHub release's text
#
# `update-manifest` builds `<dir>/rcommander-update-stable.json` for a tag `vX.Y.Z`,
# `rcommander-update-beta.json` for `vX.Y.Z-beta.N`, over the assets in <dir>, with `rc-pack update
# manifest build` and checks it with `rc-pack update manifest verify`. The download base is the
# tag's GitHub release (GITHUB_REPOSITORY, default degoya/rCommander). The notes are the version's
# section of RELEASE-NOTES.md, which the Updates page shows with the offer. `schema_change` is what
# scripts/update-schema-change.sh says of the tag, always handed to rc-pack: a manifest that does
# not say reads as a change (`UpdateManifest::changes_schema`). The script needs the earlier tags
# for it, so release.yml checks out the whole history there; a shallow checkout gives `true`.
#
# `release-text` writes into <file> what the GitHub release says: the points of the version's
# `## X.Y.Z` section of RELEASE-NOTES.md without its heading; a beta's own `## X.Y.Z-beta.N`
# section, else its release's `## X.Y.Z` (the docs gate allows both). No such section with text:
# `::error::` and exit 1.
#
# Environment:
#   RCOMMANDER_UPDATE_SIGNING_KEY  the update key: its content (the repository secret) or the
#                                  path of its file. Unset or empty: an error (exit 1) when
#                                  GITHUB_REPOSITORY is degoya/rCommander - the real release never
#                                  goes out without a manifest; anywhere else (a fork, a local
#                                  run) a warning, no manifest, exit 0.
#   RC_PACK                        the packager (default `rc-pack` on PATH)
#
# The key never reaches a log or the command line: its content goes into a file only this user can
# read, removed on exit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/release-tag.sh
source "$ROOT/scripts/lib/release-tag.sh"
# shellcheck source=lib/changelog.sh
source "$ROOT/scripts/lib/changelog.sh"

# The repository whose releases the installations read: there a release without a manifest is an
# error.
RELEASE_REPOSITORY="degoya/rCommander"

usage() {
    echo "usage: scripts/release-publish.sh require-key | update-manifest <dir> <tag> | release-text <tag> <file>" >&2
    exit 2
}

# Whether the update key is there: 0 yes; 1 after an `::error::` in the release repository; 3 after
# a `::warning::` anywhere else.
require_key() {
    [[ -z "${RCOMMANDER_UPDATE_SIGNING_KEY:-}" ]] || return 0
    if [[ "${GITHUB_REPOSITORY:-}" == "$RELEASE_REPOSITORY" ]]; then
        echo "::error::RCOMMANDER_UPDATE_SIGNING_KEY is not set; a release of $RELEASE_REPOSITORY carries its update manifest"
        return 1
    fi
    echo "::warning::RCOMMANDER_UPDATE_SIGNING_KEY is not set; this release publishes no update manifest"
    return 3
}

update_manifest() {
    local dir="$1" tag="$2" version channel key manifest schema_change pack="${RC_PACK:-rc-pack}"
    local secret="${RCOMMANDER_UPDATE_SIGNING_KEY:-}"
    version="${tag#v}"
    if [[ "$tag" != v* ]] || ! rc_release_version "$version"; then
        echo "release-publish: '$tag' is not a release tag (vX.Y.Z or vX.Y.Z-beta.N)" >&2
        exit 2
    fi
    [[ -d "$dir" ]] || { echo "release-publish: $dir is not a directory" >&2; exit 2; }
    require_key || { [[ $? -eq 3 ]] && exit 0 || exit 1; }
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
    # Its reason goes to the log; anything but a plain `false` is a change.
    schema_change="$("$ROOT/scripts/update-schema-change.sh" "$tag")" || schema_change=true
    [[ "$schema_change" == false ]] || schema_change=true
    manifest="$dir/rcommander-update-$channel.json"
    "$pack" update manifest build --channel "$channel" --version "$version" --assets "$dir" \
        --base-url "https://github.com/${GITHUB_REPOSITORY:-degoya/rCommander}/releases/download/$tag/" \
        --key "$key" --notes "$ROOT/RELEASE-NOTES.md" --schema-change "$schema_change" --out "$manifest"
    "$pack" update manifest verify "$manifest" --assets "$dir"
    echo "release-publish: $manifest for $version ($channel, schema change: $schema_change), verified"
}

release_text() {
    local version="${1#v}" file="$2" heading
    rc_release_version "$version" || { echo "::error::$1 is no release tag"; return 1; }
    for heading in "## $version" "## $(rc_release_base "$version")"; do
        { rc_doc_section "$ROOT/RELEASE-NOTES.md" "$heading" || true; } | sed 1d > "$file"
        if grep -q '[^[:space:]]' "$file"; then return 0; fi
    done
    echo "::error::RELEASE-NOTES.md has no section ## $version (nor ## $(rc_release_base "$version"))"
    return 1
}

case "${1:-}" in
    release-text)
        [[ $# -eq 3 ]] || usage
        release_text "$2" "$3"
        ;;
    require-key)
        [[ $# -eq 1 ]] || usage
        require_key || { [[ $? -eq 3 ]] && exit 0 || exit 1; }
        ;;
    update-manifest)
        [[ $# -eq 3 ]] || usage
        update_manifest "$2" "$3"
        ;;
    *) usage ;;
esac
