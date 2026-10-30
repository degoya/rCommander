#!/usr/bin/env bash
#
# scripts/update-website.sh against a throwaway site repository with a stand-in `pnpm` on PATH:
# a missing, malformed or pre-release version is refused; the wrong branch, a dirty checkout and
# a wiki page the public wiki lacks stop it before anything changes; a release sets
# release.json's version and date and site.json's version and commits "Release X.Y.Z"; a re-run
# keeps the date and commits nothing; a failing site test or a download page without the
# version's assets puts the data files back; --push pushes the branch to the site's origin.
#
# Pure git, bash and python: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/update-website.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
SITE="$SCRATCH/site"
WIKI="$SCRATCH/wiki"
export RC_WEBSITE_DIR="$SITE" RC_PUBLIC_WIKI_DIR="$WIKI" RC_WEBSITE_BRANCH=main \
    GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
    GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
SCRIPT="$ROOT/scripts/update-website.sh"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

# The stand-in pnpm: `test` fails when FAKE_PNPM_TEST_FAILS is set; `run generate` writes a
# download page linking the version in release.json (or v0.0.0 under FAKE_PNPM_WRONG_PAGE).
mkdir -p "$SCRATCH/bin"
cat > "$SCRATCH/bin/pnpm" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
    test) [[ -z "${FAKE_PNPM_TEST_FAILS:-}" ]] ;;
    "run generate")
        version="$(python3 -I -c 'import json; print(json.load(open("app/data/release.json"))["version"])')"
        [[ -z "${FAKE_PNPM_WRONG_PAGE:-}" ]] || version=0.0.0
        mkdir -p .output/public/download
        echo "<a href=\"https://github.com/degoya/rCommander/releases/download/v$version/x.zip\">" \
            > .output/public/download/index.html ;;
    *) echo "unexpected pnpm $*" >&2; exit 1 ;;
esac
SH
chmod +x "$SCRATCH/bin/pnpm"
export PATH="$SCRATCH/bin:$PATH"

mkdir -p "$SITE/app/data" "$SITE/app/utils" "$SITE/node_modules" "$WIKI/getting-started"
printf '%s\n' '{' '  "version": "0.8.0",' '  "date": "2026-10-01",' '  "repo": "degoya/rCommander"' '}' \
    > "$SITE/app/data/release.json"
printf '%s\n' '{' '  "name": "rCommander",' '  "version": "0.8.0"' '}' > "$SITE/app/data/site.json"
printf '%s\n' "const docs = { installation: wiki('installation'), firstSteps: wiki('first-steps') }" \
    > "$SITE/app/utils/release.ts"
printf '%s\n' node_modules .output > "$SITE/.gitignore"
git -C "$SITE" init -q -b main
git -C "$SITE" add -A
git -C "$SITE" commit -qm start
git init -q --bare "$SCRATCH/origin.git"
git -C "$SITE" remote add origin "$SCRATCH/origin.git"
touch "$WIKI/getting-started/installation.md"

field() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$SITE/app/data/$1" "$2"; }
subject() { git -C "$SITE" log -1 --format=%s; }

run_status bash "$SCRIPT"
expect_status "no version is refused" 2
run_status bash "$SCRIPT" v0.9
expect_status "a malformed version is refused" 2
run_status bash "$SCRIPT" 0.9.0-beta.1
expect_status "a pre-release is refused" 2
expect_output "a pre-release says why" "is a pre-release"

git -C "$SITE" checkout -q -b other
run_status bash "$SCRIPT" 0.9.0
expect_status "the wrong branch stops it" 1
expect_output "the wrong branch is named" "the site is on 'other'"
git -C "$SITE" checkout -q main

echo dirty > "$SITE/stray.txt"
run_status bash "$SCRIPT" 0.9.0
expect_status "a dirty checkout stops it" 1
rm "$SITE/stray.txt"

run_status bash "$SCRIPT" 0.9.0
expect_status "a wiki page the public wiki lacks stops it" 1
expect_output "the missing page is named" "the wiki page 'first-steps'"
expect "the data file is untouched by a stop" "0.8.0" "$(field release.json version)"
touch "$WIKI/getting-started/first-steps.md"

FAKE_PNPM_TEST_FAILS=1 run_status bash "$SCRIPT" 0.9.0
expect_true "a failing site test fails the run" '[[ $status -ne 0 ]]'
expect "a failing site test puts the data files back" "" "$(git -C "$SITE" status --porcelain)"

FAKE_PNPM_WRONG_PAGE=1 run_status bash "$SCRIPT" 0.9.0
expect_status "a download page without the version's assets fails the run" 1
expect_output "it says so" "does not link the v0.9.0 assets"
expect "the data files are back after it" "" "$(git -C "$SITE" status --porcelain)"

run_status bash "$SCRIPT" 0.9.0
expect_status "a release runs through" 0
expect "release.json names the version" "0.9.0" "$(field release.json version)"
expect "release.json takes today's date" "$(date +%F)" "$(field release.json date)"
expect "site.json follows the version" "0.9.0" "$(field site.json version)"
expect "the repository stays" "degoya/rCommander" "$(field release.json repo)"
expect "the release is committed" "Release 0.9.0" "$(subject)"
expect "nothing is left uncommitted" "" "$(git -C "$SITE" status --porcelain)"
expect_output "without --push nothing is pushed" "not pushed"
expect "the origin has nothing yet" "" "$(git -C "$SCRATCH/origin.git" branch --list main)"

python3 -I - "$SITE/app/data/release.json" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path))
data["date"] = "2000-01-01"  # never today, or the commit below has nothing to commit
json.dump(data, open(path, "w"), indent=2)
open(path, "a").write("\n")
PY
git -C "$SITE" commit -qam "a release date set by hand"
run_status bash "$SCRIPT" 0.9.0 --push
expect_status "a re-run of the same version runs through" 0
expect "a re-run keeps the release date" "2000-01-01" "$(field release.json date)"
expect_output "a re-run commits nothing" "nothing to commit"
expect "--push pushes the branch" "$(git -C "$SITE" rev-parse HEAD)" \
    "$(git -C "$SCRATCH/origin.git" rev-parse main)"

finish_tests update-website
