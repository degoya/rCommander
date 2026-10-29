#!/usr/bin/env bash
#
# Brings the website (rcommander.net) to a release (RC-100-46), as rDownloader does it.
#
# The site is its own repository (~/projects/rcommander-website, Nuxt, statically generated).
# Every release fact it shows - the version, the release date, the tag, the download assets and
# their SHA256SUMS, the handbook links - derives from app/data/release.json, and this script sets
# it: the version, today's date (kept when the file already names this version with a date, so a
# re-run does not move the release date; an empty date is the placeholder of a version prepared
# before its tag and is filled) and the repository, which stays. The `version` of
# app/data/site.json follows. Then it checks that every public wiki page the site links to exists
# in the exported wiki, runs the site's tests, generates it, checks that the generated download
# page links this version's assets, and commits "Release <version>" in the site repository.
#
# The site's feature texts are not touched: where a release changes what the site describes,
# that is written by hand, like the wiki.
#
# A pre-release (`X.Y.Z-beta.N`) is refused: the site offers the stable release.
#
# Nothing leaves this machine without --push, and even then only the commit. The script never
# deploys: scripts/deploy.py in the site repository publishes .output/public/ on the owner's word,
# and the last line names that directory.
#
# Usage:
#   scripts/update-website.sh 0.9.0           # set, test, generate, commit
#   scripts/update-website.sh 0.9.0 --push    # ... and push the site's branch
#
# Environment:
#   RC_WEBSITE_DIR          the site's checkout (default: ~/projects/rcommander-website)
#   RC_WEBSITE_BRANCH       the branch a release is committed on (default: main)
#   RC_PUBLIC_WIKI_DIR      the public wiki's clone, as for export-wiki.sh
#                           (default: ~/projects/rCommander-public.wiki)
#
# Takes no build lock: it compiles no Rust; pnpm runs in the site's checkout.
set -euo pipefail

SITE="${RC_WEBSITE_DIR:-$HOME/projects/rcommander-website}"
SITE_BRANCH="${RC_WEBSITE_BRANCH:-main}"
WIKI_DIR="${RC_PUBLIC_WIKI_DIR:-$HOME/projects/rCommander-public.wiki}"
DATA="app/data/release.json"
SITE_DATA="app/data/site.json"

VERSION=""
DO_PUSH=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --push) DO_PUSH=1; shift ;;
        -h|--help) sed -n '2,34p' "$0"; exit 0 ;;
        -*) echo "unknown argument: $1" >&2; exit 2 ;;
        *)
            [[ -z "$VERSION" ]] || { echo "unexpected argument: $1" >&2; exit 2; }
            VERSION="$1"; shift ;;
    esac
done
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[1-9][0-9]*)?$ ]]; then
    echo "usage: scripts/update-website.sh <X.Y.Z> [--push]" >&2
    exit 2
fi
if [[ "$VERSION" == *-beta.* ]]; then
    echo "$VERSION is a pre-release: the website stays on the last stable release until the next one" >&2
    exit 2
fi

[[ -f "$SITE/$DATA" && -f "$SITE/$SITE_DATA" ]] \
    || { echo "$SITE/$DATA or $SITE_DATA not found: is RC_WEBSITE_DIR the site's checkout?" >&2; exit 1; }
branch="$(git -C "$SITE" symbolic-ref --quiet --short HEAD || true)"
[[ "$branch" == "$SITE_BRANCH" ]] \
    || { echo "the site is on '${branch:-a detached HEAD}', a release goes on '$SITE_BRANCH' (RC_WEBSITE_BRANCH)" >&2; exit 1; }
[[ -z "$(git -C "$SITE" status --porcelain)" ]] \
    || { echo "the site's checkout has uncommitted changes; commit or remove them first" >&2; exit 1; }
[[ -d "$WIKI_DIR" ]] || { echo "the public wiki clone $WIKI_DIR is missing; run export-wiki.sh first" >&2; exit 1; }

echo "==> checking the site's handbook links against $WIKI_DIR"
missing=0
while read -r page; do
    if [[ -z "$(find "$WIKI_DIR" -path "$WIKI_DIR/.git" -prune -o -name "$page.md" -print -quit)" ]]; then
        echo "!! the site links the wiki page '$page', which the public wiki does not have" >&2
        missing=1
    fi
done < <(grep -o "wiki('[^']*')" "$SITE/app/utils/release.ts" | sed "s/^wiki('//; s/')$//")
[[ "$missing" -eq 0 ]] || exit 1

# From here on a failure puts the data files back, so that the next run finds a clean checkout.
restore() { git -C "$SITE" checkout --quiet -- "$DATA" "$SITE_DATA"; }
# shellcheck disable=SC2154  # `status` is assigned inside the trap string itself
trap 'status=$?; [[ $status -eq 0 ]] || restore; exit $status' EXIT

echo "==> setting $DATA and $SITE_DATA to $VERSION"
python3 -I - "$SITE/$DATA" "$SITE/$SITE_DATA" "$VERSION" "$(date +%F)" <<'PY'
import json
import sys

release_path, site_path, version, today = sys.argv[1:]


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def save(path, data):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


release = load(release_path)
if release.get("version") != version or not release.get("date"):
    release["date"] = today
release["version"] = version
save(release_path, release)
site = load(site_path)
site["version"] = version
save(site_path, site)
print(f"   version {release['version']}, date {release['date']}, repository {release['repo']}")
PY

cd "$SITE"
[[ -d node_modules ]] || { echo "==> pnpm install"; pnpm install --frozen-lockfile; }
echo "==> the site's tests"
pnpm test
echo "==> generating the site"
pnpm run generate
grep -q "releases/download/v$VERSION/" .output/public/download/index.html \
    || { echo "!! the generated download page does not link the v$VERSION assets" >&2; exit 1; }

if git diff --quiet -- "$DATA" "$SITE_DATA"; then
    echo "==> the site already names $VERSION; nothing to commit"
else
    git add -- "$DATA" "$SITE_DATA"
    git commit --quiet -m "Release $VERSION"
    echo "==> committed 'Release $VERSION' on $SITE_BRANCH ($(git rev-parse --short HEAD))"
fi
trap - EXIT

if [[ "$DO_PUSH" -eq 1 ]]; then
    echo "==> pushing $SITE_BRANCH"
    git push origin "$SITE_BRANCH"
else
    echo "==> not pushed; publish with: git -C $SITE push origin $SITE_BRANCH"
fi

echo "==> website built in $SITE/.output/public - deploy on the owner's word: (cd $SITE && python3 scripts/deploy.py --apply)"
