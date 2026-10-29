# shellcheck shell=bash
# shellcheck disable=SC2034  # RC_RELEASE_STEPS is read by the sourcing scripts
#
# The steps of scripts/release-pipeline.sh in the order they happen, and which of them a run
# leaves out (RC-100-32). One list, so the pipeline, its evidence gate and scripts/tag-release.sh
# never disagree on what a tag needs.
#
#   source "$ROOT/scripts/lib/release-plan.sh"
#   rc_release_step_skipped <id> <push 0|1> <prerelease 0|1>   # prints why, exit 0; else exit 1
#   rc_release_required_before <id> <push> <prerelease>         # the steps the gate wants green
#
# The order:
#   preflight            on development, a clean tree, the tag still free, the tools there, the
#                        documents (scripts/lib/release-docs.sh), check.sh --preflight
#   version-bump         scripts/set-version.sh X.Y.Z: the workspace, Cargo.lock, the SDK, and
#                        CHANGELOG.md's [Unreleased] cut into [X.Y.Z]
#   test                 scripts/check.sh --full (Linux clippy and tests, the Windows lint)
#   package-windows      scripts/package-windows.sh, as a check: the release assets themselves are
#   package-linux-x64    built by the public repository's release.yml from the tag; these prove
#   package-linux-arm64  the tree packages, locked, on every target before it is tagged
#   verify-artifacts     the three archives carry VERSION.txt naming X.Y.Z, the release profile
#                        and every bundled plugin
#   docs-gate            the documents again, after the bump cut the changelog
#   archive-jobs         scripts/archive-jobs.sh --release X.Y.Z: the finished job files move into
#                        archive/ inside the release commit
#   commit               `chore(release): X.Y.Z`, refusing build output or a symlink in the index
#   merge-main           development into main, --no-ff (not for a beta: main keeps the last stable)
#   evidence-gate        every step before it green and non-empty in this run
#   public-ci            scripts/public-ci.sh on the candidate (only with --push: it is outward)
#   tag                  scripts/tag-release.sh: vX.Y.Z, annotated with the CHANGELOG section;
#                        refused unless every step before it is green in this run
#   push                 GitLab origin: main, development, the tag (only with --push)
#   publish-public       scripts/export-public.sh, export-wiki.sh and update-website.sh; with
#                        --push they push, without it they commit locally (a beta: the export only)
RC_RELEASE_STEPS=(
    preflight version-bump test package-windows package-linux-x64 package-linux-arm64
    verify-artifacts docs-gate archive-jobs commit merge-main evidence-gate public-ci tag push
    publish-public
)

rc_release_step_skipped() {
    case "$1" in
        public-ci | push)
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
