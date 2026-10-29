# shellcheck shell=bash
#
# The documents a release needs written by hand, checked, never written (RC-100-32): a pipeline
# that wrote its own release notes would certify its own work. scripts/release-start.sh asks
# before anything detaches, the pipeline's docs-gate after the version bump; both from here.
#
#   source "$ROOT/scripts/lib/release-docs.sh"
#   rc_release_docs_check 0.9.0 [--before-bump]
#
# - CHANGELOG.md: a `## [X.Y.Z] - YYYY-MM-DD` section with entries, and `## [Unreleased]` without
#   any - an entry left there would ship without being in the version's section. Before the bump
#   (--before-bump) the section may still be missing while `[Unreleased]` has the entries the
#   bump cuts into it (scripts/set-version.sh, scripts/lib/changelog.sh).
# - RELEASE-NOTES.md: a `## X.Y.Z` section with at least one `- ` point (a beta may use its
#   release's section), and no more "No version has been released yet".
#
# Every finding is printed with what fixes it; the exit is 1 when there is any.

# The `- ` entries of the section of file $1 whose heading line is exactly $2 or, with $3 = prefix,
# begins with $2; nothing when there is no such section.
rc_section_entries() {
    awk -v heading="$2" -v mode="${3:-exact}" '
        /^## / {
            inside = (mode == "prefix") ? (index($0, heading) == 1) : ($0 == heading)
            if (inside) found = 1
            next
        }
        inside && /^- / { print }
        END { if (!found) exit 1 }
    ' "$1"
}

rc_release_docs_check() {
    local version="$1" before="${2:-}" failed=0 entries unreleased base notes
    base="$(rc_release_base "$version")"

    if [[ ! -f CHANGELOG.md ]]; then
        echo "!! CHANGELOG.md is missing" >&2
        failed=1
    else
        unreleased="$(rc_section_entries CHANGELOG.md '## [Unreleased]' || true)"
        if entries="$(rc_section_entries CHANGELOG.md "## [$version] - " prefix)"; then
            if [[ -z "$entries" ]]; then
                echo "!! CHANGELOG.md: '## [$version]' has no entries; write what the release changes" >&2
                failed=1
            elif [[ -n "$unreleased" ]]; then
                echo "!! CHANGELOG.md: '## [Unreleased]' still has $(wc -l <<< "$unreleased") entries beside '## [$version]';" \
                    "move them into the version's section" >&2
                failed=1
            else
                echo "CHANGELOG.md: '## [$version]' with $(wc -l <<< "$entries") entries, '[Unreleased]' empty"
            fi
        elif [[ "$before" == --before-bump && -n "$unreleased" ]]; then
            echo "CHANGELOG.md: $(wc -l <<< "$unreleased") entries under '[Unreleased]', cut into '[$version]' by the version bump"
        else
            echo "!! CHANGELOG.md has no '## [$version] - <date>' section$(
                [[ "$before" == --before-bump ]] && echo " and nothing under '[Unreleased]' to cut into it")" >&2
            failed=1
        fi
    fi

    if [[ ! -f RELEASE-NOTES.md ]]; then
        echo "!! RELEASE-NOTES.md is missing" >&2
        failed=1
    else
        notes="$(rc_section_entries RELEASE-NOTES.md "## $version" || true)"
        [[ -n "$notes" || "$base" == "$version" ]] || notes="$(rc_section_entries RELEASE-NOTES.md "## $base" || true)"
        if [[ -z "$notes" ]]; then
            echo "!! RELEASE-NOTES.md has no '## $version' section with '- ' points; write what a user notices" >&2
            failed=1
        else
            echo "RELEASE-NOTES.md: $(wc -l <<< "$notes") points for $version"
        fi
        if grep -qF 'No version has been released yet' RELEASE-NOTES.md; then
            echo "!! RELEASE-NOTES.md still says 'No version has been released yet'; drop the sentence" >&2
            failed=1
        fi
    fi

    [[ "$failed" -eq 0 ]]
}
