# shellcheck shell=bash
# shellcheck disable=SC2154  # the globals are release-pipeline.sh's, which sources this file
#
# The release chain's steps from docs-gate to publish-public, and the trap that leaves the
# checkout on the release branch (scripts/release-pipeline.sh; the order and the reasons:
# scripts/lib/release-plan.sh). Expects VERSION, LOG, NONCE, PRERELEASE, DO_PUSH,
# RELEASE_BRANCH, MAIN_BRANCH and the working directory at the checkout root.

# A gate, not a generator: the changelog and the release notes are judgement.
step_docs_gate() { rc_release_docs_check "$VERSION"; }

# The job files this release finished move into archive/ (--release counts the version as tagged,
# since the tag comes later); with nothing due it says so and passes.
step_archive_jobs() {
    scripts/archive-jobs.sh --release "$VERSION" || return 1
    echo "archive-jobs: done for $VERSION"
}

# Nothing built, linked or signed may enter the release commit; checked on the index before the
# commit exists. A release whose bump and documents were committed beforehand has nothing left to
# commit, which is a finished state, not an unstarted one.
step_commit() {
    local links forbidden
    git add -A || return 1
    echo "--- staged ---"
    git diff --cached --name-status
    links="$(git diff --cached --raw | awk '$2 == "120000" { print $NF }')"
    if [[ -n "$links" ]]; then
        echo "symlinks staged:" >&2
        echo "$links" >&2
        return 1
    fi
    forbidden="$(git diff --cached --name-only | grep -E '^(artifacts|target)/|\.(rcplug|pem|key)$' || true)"
    if [[ -n "$forbidden" ]]; then
        echo "build output or key material staged:" >&2
        echo "$forbidden" >&2
        return 1
    fi
    echo "symlinks, build output, keys: none staged"
    if [[ -z "$(git diff --cached --name-only)" ]]; then
        if [[ "$(scripts/set-version.sh)" == "$VERSION" ]] && grep -qF "## [$VERSION] - " CHANGELOG.md; then
            echo "nothing to commit: the version and the changelog are already on $RELEASE_BRANCH"
            git --no-pager log --oneline -1
            return 0
        fi
        echo "nothing staged, and the release content is not committed either" >&2
        return 1
    fi
    git commit -q -m "chore(release): $VERSION" || return 1
    git --no-pager show --stat --oneline HEAD
}

step_merge_main() {
    local from links
    from="$(git rev-parse --abbrev-ref HEAD)"
    echo "merging $from into $MAIN_BRANCH"
    git checkout -q "$MAIN_BRANCH" || return 1
    if ! git merge --no-ff "$from" -m "Merge branch '$from' - release $VERSION"; then
        echo "the merge conflicted; leaving $MAIN_BRANCH untouched" >&2
        git merge --abort || true
        git checkout -q "$from"
        return 1
    fi
    git --no-pager log --oneline -1
    links="$(git ls-tree -r HEAD | awk '$1 == "120000" { print $4 }')"
    if [[ -n "$links" ]]; then
        echo "the merged tree contains symlinks:" >&2
        echo "$links" >&2
        return 1
    fi
    echo "merged tree: no symlinks"
}

step_evidence_gate() {
    local -a required
    mapfile -t required < <(rc_release_required_before evidence-gate "$DO_PUSH" "$PRERELEASE")
    echo "verifying the evidence of run $NONCE in $LOG"
    rc_release_gate "${required[@]}" || { echo "refusing to tag: the evidence is incomplete" >&2; return 1; }
    echo "all ${#required[@]} steps have passing, non-empty evidence"
}

# The candidate through the public repository's CI before it is tagged: GitHub's runners check
# Windows and macOS, which this machine cannot. Outward, so only with --push.
step_public_ci() { scripts/public-ci.sh "ci/$VERSION"; }

step_tag() {
    scripts/tag-release.sh "$VERSION" || return 1
    # Not `head`: it closes the pipe, git dies of SIGPIPE and the step fails on its own summary.
    git --no-pager show --stat --oneline "v$VERSION" | sed -n '1,20p'
}

# GitLab origin. A pre-release pushes no main: main keeps the last stable release.
step_push() {
    local -a refs=("$RELEASE_BRANCH" "v$VERSION")
    [[ "$PRERELEASE" -eq 1 ]] || refs=("$MAIN_BRANCH" "${refs[@]}")
    git push origin "${refs[@]}" || return 1
    echo "pushed ${refs[*]} to origin"
}

# After `push`, so nothing reaches the public side before the private one has it. The tag in the
# public repository starts its release.yml. Without --push each script commits locally only. A
# pre-release is exported alone: the handbook and the website describe the stable release.
step_publish_public() {
    local -a push=()
    [[ "$DO_PUSH" -eq 0 ]] || push=(--push)
    scripts/export-public.sh "$VERSION" "${push[@]}" || return 1
    if [[ "$PRERELEASE" -eq 1 ]]; then
        echo "pre-release: the public wiki and the website stay on the last stable release"
        return 0
    fi
    scripts/export-wiki.sh "$VERSION" "${push[@]}" || return 1
    scripts/update-website.sh "$VERSION" "${push[@]}" || return 1
    echo "published $VERSION$([[ $DO_PUSH -eq 1 ]] && echo ' and pushed' || echo ', committed locally')"
}

# From merge-main on the checkout stands on main (the gate and the tag need HEAD there); when the
# chain ends, green or failed, an exit trap switches back so the next commit cannot land on main.
return_to_release_branch() {
    [[ "$(git rev-parse --abbrev-ref HEAD 2> /dev/null)" == "$MAIN_BRANCH" ]] || return 0
    if git checkout -q "$RELEASE_BRANCH"; then
        echo "==> the checkout is back on $RELEASE_BRANCH; $MAIN_BRANCH is merged into, never committed on"
    else
        echo "!! could not switch back to $RELEASE_BRANCH: this checkout is still on $MAIN_BRANCH." >&2
        echo "   git checkout $RELEASE_BRANCH before committing anything." >&2
    fi
}
