#!/usr/bin/env bash
#
# scripts/screenshots.sh without Windows: its arguments (a wrong one, a timeout without a
# number), where the pictures go (the artifacts of the version, the website and the wiki beside
# the main checkout or as RC_WEBSITE_DIR and RC_WIKI_DIR say), the manifest it reads (a failed
# run, a name that leaves the folder, a missing picture refuse it), the copy into the artifacts
# and the delivery, which copies nothing while one checkout is missing; and that every scene of
# scripts/screenshots/scenes.json has its alt text and no alt text is left over.
#
# Pure bash and python3: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/screenshots.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# Sourced, the script defines its functions and runs nothing.
# shellcheck source=../screenshots.sh
source "$ROOT/scripts/screenshots.sh"

parse_args
expect "a debug build by default" "0 0 600" "$RELEASE $DELIVER $TIMEOUT"
parse_args --release --deliver --timeout 120
expect "release, deliver and a timeout" "1 1 120" "$RELEASE $DELIVER $TIMEOUT"
run_status bash "$ROOT/scripts/screenshots.sh" --upload
expect_status "an unknown argument is refused" 2
expect_output "with the usage" "usage: scripts/screenshots.sh"
run_status bash "$ROOT/scripts/screenshots.sh" --timeout
expect_status "a timeout without seconds is refused" 2
run_status bash "$ROOT/scripts/screenshots.sh" --timeout soon
expect_status "a timeout that is no number is refused" 2
run_status bash "$ROOT/scripts/screenshots.sh" --timeout 0
expect_status "a timeout of nothing is refused" 2

expect "the artifacts of the version" "$ROOT/artifacts/screenshots/1.2.3" "$(artifacts_dir 1.2.3)"
MAIN="$(main_checkout)"
expect_true "the main checkout holds the git directory" '[[ -e "$MAIN/.git" ]]'
expect "the website beside the main checkout" \
    "$MAIN/../rcommander-website/public/screenshots" "$(RC_WEBSITE_DIR='' website_dir)"
expect "the wiki beside the main checkout" \
    "$MAIN/../rcommander.wiki/images/screenshots" "$(RC_WIKI_DIR='' wiki_dir)"
expect "the website as named" "/x/site/public/screenshots" "$(RC_WEBSITE_DIR=/x/site website_dir)"
expect "the wiki as named" "/x/wiki/images/screenshots" "$(RC_WIKI_DIR=/x/wiki wiki_dir)"

OUT="$SCRATCH/out"
mkdir -p "$OUT"
printf 'png-a' > "$OUT/a.png"
printf 'png-b' > "$OUT/b-2.png"
printf '{"version":"1.2.3","ok":true,"shots":[{"name":"a","file":"a.png","width":1,"height":1},{"name":"b-2","file":"b-2.png","width":1,"height":1}]}' \
    > "$OUT/screenshots.json"
run_status listed_shots "$OUT/screenshots.json" "$OUT"
expect_status "a run that took its pictures is read" 0
expect "both pictures, in order" "$(printf 'a.png\nb-2.png')" "$output"

DEST="$SCRATCH/artifacts/1.2.3"
mkdir -p "$DEST"
printf 'old' > "$DEST/gone.png"
run_status collect "$OUT/screenshots.json" "$OUT" "$DEST"
expect_status "the pictures are collected" 0
expect_true "both copied" '[[ "$(cat "$DEST/a.png")" == png-a && "$(cat "$DEST/b-2.png")" == png-b ]]'
expect_true "an old picture is gone" '[[ ! -e "$DEST/gone.png" ]]'
expect_true "the manifest beside them" '[[ -f "$DEST/screenshots.json" ]]'

printf '{"ok":false,"error":"scene x: no window","shots":[]}' > "$SCRATCH/failed.json"
run_status listed_shots "$SCRATCH/failed.json" "$OUT"
expect_status "a failed run is refused" 1
expect_output "with its reason" "the run failed: scene x: no window"
for name in '../up.png' 'a/b.png' 'A.png' 'a.jpg' '.png'; do
    printf '{"ok":true,"shots":[{"file":"%s"}]}' "$name" > "$SCRATCH/bad.json"
    run_status listed_shots "$SCRATCH/bad.json" "$OUT"
    expect_status "the name $name is refused" 1
done
printf '{"ok":true,"shots":[{"file":"missing.png"}]}' > "$SCRATCH/missing.json"
run_status listed_shots "$SCRATCH/missing.json" "$OUT"
expect_status "a missing picture is refused" 1
expect_output "and named" "missing.png is listed but missing"
printf '{"ok":true,"shots":[]}' > "$SCRATCH/empty.json"
run_status listed_shots "$SCRATCH/empty.json" "$OUT"
expect_status "a run without pictures is refused" 1

SITE="$SCRATCH/site"
WIKI="$SCRATCH/wiki"
mkdir -p "$SITE"
RC_WEBSITE_DIR="$SITE" RC_WIKI_DIR="$WIKI" run_status deliver "$DEST"
expect_status "a missing wiki refuses the delivery" 1
expect_true "and nothing reached the website" '[[ ! -e "$SITE/public/screenshots" ]]'
mkdir -p "$WIKI"
RC_WEBSITE_DIR="$SITE" RC_WIKI_DIR="$WIKI" run_status deliver "$DEST"
expect_status "the pictures are delivered" 0
expect_true "to the website" '[[ -f "$SITE/public/screenshots/a.png" && -f "$SITE/public/screenshots/b-2.png" ]]'
expect_true "to the wiki" '[[ -f "$WIKI/images/screenshots/a.png" ]]'
expect_true "the manifest stays in the artifacts" '[[ ! -e "$WIKI/images/screenshots/screenshots.json" ]]'

run_status python3 -B -I - "$ROOT/scripts/screenshots/scenes.json" "$ROOT/scripts/screenshots/alts.json" << 'EOF'
import json, sys
scenes = [scene["name"] for scene in json.load(open(sys.argv[1], encoding="utf-8"))["scenes"]]
alts = json.load(open(sys.argv[2], encoding="utf-8"))
missing = [name for name in scenes if not alts.get(name)]
extra = [name for name in alts if name not in scenes]
if missing or extra:
    sys.exit(f"without alt text: {missing}; alt text without a scene: {extra}")
EOF
expect_status "every scene has an alt text, none is left over" 0

finish_tests screenshots
