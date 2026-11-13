# shellcheck shell=bash
#
# CHANGELOG.md's version sections (owner 2026-10-25, since 0.6.0): what `## [Unreleased]` holds
# becomes `## [X.Y.Z] - YYYY-MM-DD` at every version bump, under a new, empty `## [Unreleased]`.
# scripts/set-version.sh calls it; a version that has its section already is left alone, so a
# second run changes nothing.
#
#   source "$ROOT/scripts/lib/changelog.sh"
#   rc_changelog_cut CHANGELOG.md 0.6.1 2026-10-25
#   rc_doc_section CHANGELOG.md '## [0.6.1] - ' prefix    # the section, its heading first
#   rc_doc_section RELEASE-NOTES.md '## 0.6.1'            # ... of a heading given whole

# The section of the Markdown file $1 under the `## ` heading that is exactly $2 or, with $3 =
# prefix, begins with $2: the heading line and everything up to the next `## ` heading. Nothing
# and 1 when there is no such heading. The one reader of a version's section (RC-110-33): the
# docs gate (lib/release-docs.sh), the tag's message (tag-release.sh) and the GitHub release's
# text (release-publish.sh release-text).
rc_doc_section() {
    awk -v heading="$2" -v mode="${3:-exact}" '
        /^## / {
            inside = (mode == "prefix") ? (index($0, heading) == 1) : ($0 == heading)
            if (inside) found = 1
        }
        inside { print }
        END { if (!found) exit 1 }
    ' "$1"
}

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
