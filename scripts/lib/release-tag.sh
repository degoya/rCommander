# shellcheck shell=bash
#
# The one form of a release version (RC-100-32, as rDownloader): `X.Y.Z` for a release, and
# `X.Y.Z-beta.N` for a pre-release, nothing else. The release scripts refuse every other form
# with rc_release_version and ask rc_is_prerelease what a beta leaves out (the merge into main,
# the wiki and the website).
#
#   source "$ROOT/scripts/lib/release-tag.sh"
#   rc_release_version 0.9.0-beta.2   # true; false for 0.9.0-rc.1, 0.9, v0.9.0
#   rc_is_prerelease 0.9.0-beta.2     # true; false for 0.9.0
#   rc_release_base 0.9.0-beta.2      # 0.9.0

RC_RELEASE_VERSION_PATTERN='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-beta\.[1-9][0-9]*)?$'
rc_release_version() { [[ "${1:-}" =~ $RC_RELEASE_VERSION_PATTERN ]]; }
rc_is_prerelease() { rc_release_version "${1:-}" && [[ "$1" == *-beta.* ]]; }
rc_release_base() { printf '%s\n' "${1%%-*}"; }
