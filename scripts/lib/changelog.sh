# shellcheck shell=bash
#
# CHANGELOG.md's version sections (owner 2026-10-25, since 0.6.0): what `## [Unreleased]` holds
# becomes `## [X.Y.Z] - YYYY-MM-DD` at every version bump, under a new, empty `## [Unreleased]`.
# scripts/set-version.sh calls it; a version that has its section already is left alone, so a
# second run changes nothing.
#
#   source "$ROOT/scripts/lib/changelog.sh"
#   rc_changelog_cut CHANGELOG.md 0.6.1 2026-10-25

# Cuts `## [Unreleased]` of the changelog `$1` into version `$2` of date `$3`; 1 without an
# `## [Unreleased]` heading.
rc_changelog_cut() {
    local file="$1" version="$2" date="$3"
    if grep -qF -- "## [$version] - " "$file"; then
        return 0
    fi
    if ! grep -qx '## \[Unreleased\]' "$file"; then
        echo "!! $file has no '## [Unreleased]' heading to cut $version from" >&2
        return 1
    fi
    awk -v heading="## [$version] - $date" '
        !done && $0 == "## [Unreleased]" { print; print ""; print heading; done = 1; next }
        { print }
    ' "$file" > "$file.cut" && mv "$file.cut" "$file"
}
