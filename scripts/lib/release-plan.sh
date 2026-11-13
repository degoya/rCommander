# shellcheck shell=bash
# shellcheck disable=SC2034  # RC_RELEASE_STEPS and RC_RELEASE_PLAN are read by the sourcing scripts
#
# The steps of scripts/release-pipeline.sh in the order they happen, and which of them a run
# leaves out (RC-100-32). One list, so the pipeline, its evidence gate and scripts/tag-release.sh
# never disagree on what a tag needs.
#
#   source "$ROOT/scripts/lib/release-plan.sh"
#   rc_release_step_skipped <id> <push 0|1> <prerelease 0|1>   # prints why, exit 0; else exit 1
#   rc_release_required_before <id> <push> <prerelease>         # the steps the gate wants green
#   rc_release_tree_bound <id>                                  # whether <id> runs on the commit's tree
#
# The order (RC-110-31: the tree is committed before anything is tested, so what is tagged is what
# was tested; from `commit` to `tag` no step changes the tree, and the gate holds each to it):
#   preflight            on development, a clean tree, the tag still free, the tools there, the
#                        documents (scripts/lib/release-docs.sh), check.sh --preflight
#   version-bump         scripts/set-version.sh X.Y.Z: the workspace, Cargo.lock, the SDK, and
#                        CHANGELOG.md's [Unreleased] cut into [X.Y.Z]
#   docs-gate            the documents again, after the bump cut the changelog - before anything
#                        is built, so a missing section costs seconds
#   archive-jobs         scripts/archive-jobs.sh --release X.Y.Z: the finished job files move into
#                        archive/ inside the release commit
#   commit               `chore(release): X.Y.Z`, refusing build output or a symlink in the index;
#                        the last step that writes into the tree
#   public-ci-start      scripts/public-ci.sh on the release commit, in the background beside the
#                        steps below (only with --push: it is outward)
#   test                 scripts/check.sh --full (Linux clippy and tests, the Windows lint)
#   package-windows      scripts/package-windows.sh, as a check: the release assets themselves are
#   package-linux-x64    built by the public repository's release.yml from the tag; these prove
#                        the commit packages, locked, before it is tagged. Linux arm64 is not
#                        packaged here (owner, 2026-11-13): GitHub builds and tests it natively
#   verify-artifacts     the two archives carry VERSION.txt naming X.Y.Z, the release profile
#                        and every bundled plugin
#   merge-main           development into main, --no-ff (not for a beta: main keeps the last stable)
#   public-ci            the result of the run public-ci-start began, then scripts/public-ci.sh
#                        on HEAD, which a tree already green passes at once (only with --push)
#   stage-public         scripts/export-public.sh, export-wiki.sh and update-website.sh without
#                        --push: every check of the public side (links, names, gitleaks, the
#                        site's tests) and the local commits, before the tag (a beta: the export only)
#   evidence-gate        every step before it green and non-empty in this run, and every one from
#                        `commit` on recorded on HEAD's tree
#   tag                  scripts/tag-release.sh: vX.Y.Z, annotated with the CHANGELOG section;
#                        refused unless the same gate holds
#   push                 GitLab origin: main, development, the tag (only with --push)
#   publish-public       the three scripts of stage-public again with --push: the same trees,
#                        pushed (only with --push)
RC_RELEASE_STEPS=(
    preflight version-bump docs-gate archive-jobs commit public-ci-start test package-windows
    package-linux-x64 verify-artifacts merge-main public-ci stage-public
    evidence-gate tag push publish-public
)
# The number of this order, written into a run's header. A log of another order is not resumed and
# not tagged from: before 2 `test` and the packages ran ahead of the release commit, so their
# markers say nothing about the tree the tag goes on. Still 2 without `package-linux-arm64`
# (2026-11-13): a step taken out moves nothing, so a run begun with it resumes - its arm64 marker
# is a line no step asks for.
RC_RELEASE_PLAN=2

rc_release_step_skipped() {
    case "$1" in
        public-ci-start | public-ci | push | publish-public)
            [[ "$2" -eq 0 ]] || return 1
            echo "skipped: --push not given"
            ;;
        merge-main)
            [[ "$3" -eq 1 ]] || return 1
            echo "skipped: a pre-release is not merged into main"
            ;;
        *) return 1 ;;
    esac
}

rc_release_required_before() {
    local id
    for id in "${RC_RELEASE_STEPS[@]}"; do
        [[ "$id" != "$1" ]] || return 0
        rc_release_step_skipped "$id" "$2" "$3" > /dev/null || printf '%s\n' "$id"
    done
}

# Whether step $1 runs on the release commit's tree: every step from `commit` up to the tag. Its
# marker must record HEAD's tree (scripts/lib/release-evidence.sh).
rc_release_tree_bound() {
    local id bound=1
    for id in "${RC_RELEASE_STEPS[@]}"; do
        [[ "$id" != commit ]] || bound=0
        [[ "$id" != tag ]] || bound=1
        [[ "$id" != "$1" ]] || return "$bound"
    done
    return 1
}
