# shellcheck shell=bash
# shellcheck disable=SC2154  # the globals are release-pipeline.sh's, which sources this file
#
# The release chain's steps around the release commit and the public side - docs-gate to
# public-ci-start, merge-main to publish-public - and the trap that leaves the checkout on the
# release branch (scripts/release-pipeline.sh; the order and the reasons:
# scripts/lib/release-plan.sh). Expects VERSION, LOG, NONCE, PRERELEASE, DO_PUSH, ROOT,
# RELEASE_BRANCH, MAIN_BRANCH and the working directory at the checkout root.
#
# Environment:
#   RC_RELEASE_CI_POLL  seconds between two looks at the public CI run started after the commit
#                       (30)

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
# commit, which is a finished state, not an unstarted one. The last step that writes into the
# tree (RC-110-31): everything after it - the tests, the packages, the public CI, the tag - sees
# this commit's tree, and run_step turns a step red that leaves another.
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
# Windows and macOS, which this machine cannot. Outward, so only with --push. Started in the
# background right after the release commit (RC-110-31), beside the tests and the packages, in a
# session of its own with nothing of the step's output attached - the step's tee would wait for
# it. Its files lie beside the evidence log, named by the commit: <base>.log, .pid, .exit; the
# `current` file names the run the public-ci step reads.
rc_release_ci_current() { printf '%s/artifacts/release-public-ci-%s.current\n' "$ROOT" "$VERSION"; }

# Stops the background run whose files start with $1, when it still waits: after a change of the
# tree its public branch is exported anew, which cancels its run on GitHub anyway.
rc_release_ci_stop() {
    local pid
    [[ -s "$1.pid" && ! -f "$1.exit" ]] || return 0
    pid="$(cat "$1.pid")"
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 0
    kill -0 "$pid" 2> /dev/null || return 0
    echo "stopping the earlier public CI wait (PID $pid, ${1##*/})"
    kill -- "-$pid" 2> /dev/null || kill "$pid" 2> /dev/null || true
}

step_public_ci_start() {
    local commit base current
    commit="$(git rev-parse HEAD)" || return 1
    base="$ROOT/artifacts/release-public-ci-$VERSION-${commit:0:12}"
    current="$(rc_release_ci_current)"
    [[ ! -s "$current" ]] || rc_release_ci_stop "$(cat "$current")"
    rm -f "$base.pid" "$base.exit"
    printf '%s\n' "$base" > "$current"
    # shellcheck disable=SC2016  # expanded by the background shell
    RC_CI_BASE="$base" setsid bash -c 'printf "%s\n" "$$" > "$RC_CI_BASE.pid"
        status=0
        scripts/public-ci.sh "$1" --ref "$2" > "$RC_CI_BASE.log" 2>&1 || status=$?
        printf "%s\n" "$status" > "$RC_CI_BASE.exit"' _ "ci/$VERSION" "$commit" > /dev/null 2>&1 < /dev/null &
    # The run writes its own PID; setsid may fork, so `$!` is not guaranteed to be it.
    for _ in $(seq 1 50); do
        [[ -s "$base.pid" ]] && break
        sleep 0.1
    done
    [[ -s "$base.pid" ]] || { echo "the public CI did not start; see $base.log" >&2; return 1; }
    echo "the public CI of ${commit:0:12} (tree $(git rev-parse --short=12 'HEAD^{tree}')) runs beside the checks:"
    echo "    scripts/public-ci.sh ci/$VERSION --ref $commit, PID $(cat "$base.pid")"
    echo "    log: $base.log; the public-ci step reads its result before the tag"
}

# The result of the run public-ci-start began, waited for while it still goes: red stops the chain
# before the tag, as it always did. A result is read once, so a --resume after a red run starts
# the CI afresh. Then scripts/public-ci.sh on HEAD, the commit the tag goes on: its tree is green
# already when it is the release commit's (after merge-main it is, unless main carried something
# of its own), and nothing runs; any other tree is run here.
step_public_ci() {
    local current base pid code
    current="$(rc_release_ci_current)"
    if [[ -s "$current" ]]; then
        base="$(cat "$current")"
        pid="$(cat "$base.pid" 2> /dev/null || true)"
        if [[ ! -f "$base.exit" && "$pid" =~ ^[1-9][0-9]*$ ]] && kill -0 "$pid" 2> /dev/null; then
            echo "waiting for the public CI started after the commit (PID $pid, $base.log)"
            while [[ ! -f "$base.exit" ]] && kill -0 "$pid" 2> /dev/null; do
                sleep "${RC_RELEASE_CI_POLL:-30}"
            done
        fi
        rm -f "$current"
        if [[ -f "$base.log" ]]; then
            echo "--- the run started after the commit (${base##*/}.log) ---"
            cat "$base.log"
            echo "--- its end ---"
        fi
        if [[ -f "$base.exit" ]]; then
            code="$(cat "$base.exit")"
            [[ "$code" =~ ^[0-9]+$ ]] || code=1
            if [[ "$code" -ne 0 ]]; then
                echo "the public CI started after the commit is red (exit $code); nothing is tagged" >&2
                return "$code"
            fi
        else
            echo "the run started after the commit left no result (stopped?); running the CI here"
        fi
    fi
    scripts/public-ci.sh "ci/$VERSION"
}

# The public side before the tag (RC-110-31): the export, the handbook and the website through
# every check they have - links, private names, gitleaks, the site's tests - and committed into
# their local clones, nothing pushed. What can refuse the release's tree refuses it here, where
# the tree may still change; after the tag publish-public only pushes. The export is of HEAD, the
# commit the tag goes on. A pre-release is exported alone: the handbook and the website describe
# the stable release.
step_stage_public() {
    scripts/export-public.sh "$VERSION" --ref HEAD || return 1
    if [[ "$PRERELEASE" -eq 1 ]]; then
        echo "pre-release: the public wiki and the website stay on the last stable release"
        return 0
    fi
    scripts/export-wiki.sh "$VERSION" || return 1
    scripts/update-website.sh "$VERSION" || return 1
    echo "staged $VERSION for the public repository, its wiki and the website; nothing pushed"
}

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

# After `push`, so nothing reaches the public side before the private one has it; only with
# --push. The three scripts of stage-public again: each finds its tree staged and pushes it (the
# export, now of the tag, is the same commit). The tag in the public repository starts its
# release.yml. Nothing here needs the tree to change, so a --resume after the tag goes through.
step_publish_public() {
    scripts/export-public.sh "$VERSION" --push || return 1
    if [[ "$PRERELEASE" -eq 1 ]]; then
        echo "pre-release: the public wiki and the website stay on the last stable release"
        return 0
    fi
    scripts/export-wiki.sh "$VERSION" --push || return 1
    scripts/update-website.sh "$VERSION" --push || return 1
    echo "published $VERSION and pushed"
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
