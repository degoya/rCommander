#!/usr/bin/env bash
#
# Exports a release to the public repository (RC-100-30). Copied and adapted from rDownloader's
# scripts/export-public.sh at commit 26c8592a6.
#
# Development happens in the private repository; github.com/degoya/rCommander carries one fresh
# commit per release and no history. This script builds that commit: it takes `git archive` of
# the release tag, drops what scripts/public-exclude.txt names, refuses a link from what is left
# into what was dropped and a private name (scripts/public-names.txt) outside
# scripts/public-names-allow.txt, scans the rest with gitleaks, lists every remaining reference
# to the internal planning as a warning, and replaces the tree of a local clone of the public
# repository with it - never touching the clone's `.git`. The commit is "Release <version>",
# authored by the git identity of this repository, and tagged v<version>.
#
# Nothing leaves this machine without --push, and every push is the owner's word: the public
# repository is what strangers read, so publishing it is a step somebody takes on purpose, not a
# side effect of exporting. No push here is a force push but --branch's, on a ci/ branch.
#
# Usage:
#   scripts/export-public.sh 0.9.0                     # export v0.9.0 into the clone, commit, tag
#   scripts/export-public.sh 0.9.0 --push              # ... and push main and the tag
#   scripts/export-public.sh 0.9.0 --ref <commit>      # export another ref than v0.9.0
#   scripts/export-public.sh 0.9.0 --branch ci/0.9.0   # an unreleased export (default ref: HEAD),
#                                                      # committed to that branch and pushed there
#   scripts/export-public.sh 0.9.0 --branch ci/0.9.0 --skip-push-ci
#                                                      # ... with no CI started by the push itself
#
# A pre-release (`X.Y.Z-beta.N`) is exported like a release: the commit "Release <version>" on
# main, tagged there; the public main and its README show the beta until the stable release
# replaces it.
#
# The user handbook is not part of this tree; it goes to the repository's GitHub wiki through
# scripts/export-wiki.sh.
#
# --branch is for CI: it lets the public workflows run on a tree before it is a release. It
# starts the branch afresh from the clone's main each time, commits without a tag, and
# force-pushes the branch, because a re-export replaces the previous one. It pushes by itself -
# naming a branch is the explicit request - and only a branch under ci/, never main or a tag.
#
# --skip-push-ci puts `[skip ci]` into that commit's message, which GitHub honours for push
# events only: for a run started by hand for fewer platforms, so the push does not start a second.
#
# Environment:
#   RC_PUBLIC_DIR     the local clone (default: ~/projects/rCommander-public)
#   RC_PUBLIC_REMOTE  where to clone it from when missing (default: git@github.com:degoya/rCommander.git)
#   GITLEAKS          the gitleaks binary (default: `gitleaks` on PATH; nothing else is looked for)
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/public.sh
source "$ROOT/scripts/lib/public.sh"
cd "$ROOT"

PUBLIC_DIR="${RC_PUBLIC_DIR:-$HOME/projects/rCommander-public}"
PUBLIC_REMOTE="${RC_PUBLIC_REMOTE:-git@github.com:degoya/rCommander.git}"
PUBLIC_MAIN="main"
EXCLUDE_LIST="$ROOT/scripts/public-exclude.txt"

VERSION=""
REF=""
BRANCH=""
DO_PUSH=0
SKIP_PUSH_CI=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --push) DO_PUSH=1; shift ;;
        --ref) REF="${2:?--ref needs a ref}"; shift 2 ;;
        --branch) BRANCH="${2:?--branch needs a branch name}"; shift 2 ;;
        --skip-push-ci) SKIP_PUSH_CI=1; shift ;;
        -h|--help) sed -n '2,47p' "$0"; exit 0 ;;
        -*) echo "unknown argument: $1" >&2; exit 2 ;;
        *)
            [[ -z "$VERSION" ]] || { echo "unexpected argument: $1" >&2; exit 2; }
            VERSION="$1"; shift ;;
    esac
done

if [[ -z "$VERSION" ]]; then
    echo "usage: scripts/export-public.sh <version> [--push] [--ref <ref>] [--branch ci/<name> [--skip-push-ci]]" >&2
    exit 2
fi
if ! rc_public_version "$VERSION"; then
    echo "not a release version: $VERSION (X.Y.Z, or X.Y.Z-beta.N for a pre-release)" >&2
    exit 2
fi
if [[ "$SKIP_PUSH_CI" -eq 1 && -z "$BRANCH" ]]; then
    echo "--skip-push-ci goes with --branch" >&2
    exit 2
fi
if [[ -n "$BRANCH" ]]; then
    [[ "$DO_PUSH" -eq 0 ]] || { echo "--branch pushes by itself; drop --push" >&2; exit 2; }
    [[ "$BRANCH" == ci/?* ]] || { echo "--branch takes a ci/ branch: $BRANCH" >&2; exit 2; }
    git check-ref-format --branch "$BRANCH" > /dev/null \
        || { echo "not a branch name: $BRANCH" >&2; exit 2; }
fi

TAG="v$VERSION"
if [[ -z "$REF" ]]; then
    if [[ -n "$BRANCH" ]]; then REF="HEAD"; else REF="$TAG"; fi
fi
COMMIT="$(git rev-parse --verify --quiet "$REF^{commit}")" \
    || { echo "no such commit in this repository: $REF" >&2; exit 1; }

# --- the tools, before anything is written ---------------------------------------------------

rc_public_find_gitleaks
rc_public_find_author "$ROOT"

# --- the tree --------------------------------------------------------------------------------

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/rc-public-export.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT

echo "==> exporting $REF (${COMMIT:0:12}) to a staging tree"
git archive --format=tar "$COMMIT" | tar -x -C "$STAGE"

rc_public_exclude "$STAGE" "$EXCLUDE_LIST"

# What stays must not point at what went: a README link into docs/ would be a 404 on GitHub.
rc_public_check_links "$STAGE" "$EXCLUDE_LIST"
rc_public_check_names "$STAGE"

# From inside the tree, so that its own .gitleaks.toml applies and its paths match the ones CI
# sees in the public repository.
rc_public_scan "$STAGE"

echo "==> references to the internal planning that remain (a warning, not a refusal)"
warnings="$(cd "$STAGE" && { grep -rnE 'roadmap/jobs|AGENTS\.md' . || true; })"
if [[ -n "$warnings" ]]; then
    echo "WARNING: $(wc -l <<< "$warnings") line(s) in the export name an internal path:"
    while IFS= read -r line; do echo "    $line"; done <<< "$warnings"
else
    echo "    none"
fi

# --- the clone -------------------------------------------------------------------------------

# The very first export meets an empty repository: no main anywhere, HEAD unborn.
rc_public_prepare_clone "$PUBLIC_DIR" "$PUBLIC_REMOTE" "$PUBLIC_MAIN"

if [[ -n "$BRANCH" ]]; then
    if pub rev-parse --verify --quiet HEAD > /dev/null; then
        pub checkout --quiet -B "$BRANCH"
    else
        pub symbolic-ref HEAD "refs/heads/$BRANCH"
    fi
fi

rc_public_replace_tree "$PUBLIC_DIR" "$STAGE"

commit_as() {
    git -C "$PUBLIC_DIR" -c user.name="$AUTHOR_NAME" -c user.email="$AUTHOR_EMAIL" "$@"
}

if [[ -n "$BRANCH" ]]; then
    message="Unreleased export of ${COMMIT:0:12} (towards $VERSION)"
    [[ "$SKIP_PUSH_CI" -eq 0 ]] || message+=" [skip ci]"
    commit_as commit --quiet --allow-empty -m "$message"
    echo "==> committed $(pub rev-parse --short HEAD) on $BRANCH; pushing it"
    pub push --force origin "$BRANCH"
    pub checkout --quiet "$PUBLIC_MAIN" 2> /dev/null || true
    echo "==> pushed $BRANCH"
    exit 0
fi

if pub rev-parse --verify --quiet "refs/tags/$TAG" > /dev/null; then
    # A resumed release runs this again; the same tree under the same tag is done. Either way
    # the clone goes back to its own HEAD: the replaced tree was only the comparison.
    if pub diff --cached --quiet "$TAG"; then
        pub reset --quiet --hard
        echo "==> $TAG is already exported with exactly this tree"
    else
        pub reset --quiet --hard
        echo "$TAG already exists in the public repository with a different tree" >&2
        exit 1
    fi
else
    commit_as commit --quiet --allow-empty -m "Release $VERSION"
    commit_as tag -a "$TAG" -m "Release $VERSION"
    echo "==> committed $(pub rev-parse --short HEAD) on $PUBLIC_MAIN and tagged $TAG"
fi

if [[ "$DO_PUSH" -eq 1 ]]; then
    pub push origin "$PUBLIC_MAIN"
    pub push origin "$TAG"
    echo "==> pushed $PUBLIC_MAIN and $TAG to $PUBLIC_REMOTE"
else
    echo "==> not pushed. Publish with: git -C $PUBLIC_DIR push origin $PUBLIC_MAIN $TAG"
fi
