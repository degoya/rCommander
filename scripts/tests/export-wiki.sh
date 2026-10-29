#!/usr/bin/env bash
#
# scripts/export-wiki.sh against a scratch wiki (RC-100-30, after rDownloader's test at
# 26c8592a6): a private page and its sidebar line stay behind, a private section is cut out,
# and every way of leaking one - a link to the page, a link to a heading inside the section, a
# marker that does not pair up - refuses the export before anything reaches the clone, as do a
# private name, a link into docs/ of the repository and two pages with one base name. Nothing is
# pushed without --push; a pre-release is refused.
#
# Local only: the "public" wiki is a bare repository in the scratch directory, gitleaks a stub,
# the private names a made-up list beside a copy of the scripts.
#
#   scripts/tests/export-wiki.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

# The scripts, with their own lists: the export reads the private names beside itself.
TOOLS="$SCRATCH/tools"
mkdir -p "$TOOLS/scripts/lib"
cp "$ROOT/scripts/export-wiki.sh" "$TOOLS/scripts/"
cp "$ROOT/scripts/lib/public.sh" "$ROOT/scripts/lib/public-links.py" "$ROOT/scripts/lib/public-names.py" \
    "$TOOLS/scripts/lib/"
HOST="private.example""-host.test"
printf '%s\n' 'docs/' > "$TOOLS/scripts/public-exclude.txt"
printf '%s\n' "${HOST//./\\.}" > "$TOOLS/scripts/public-names.txt"
printf '# none\n' > "$TOOLS/scripts/public-names-allow.txt"
git -C "$TOOLS" init -q

export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid
export TMPDIR="$SCRATCH"
export RC_WIKI="$SCRATCH/wiki"
export RC_PUBLIC_WIKI_DIR="$SCRATCH/public-wiki"
export RC_PUBLIC_WIKI_REMOTE="$SCRATCH/public-wiki.git"
export GITLEAKS="$SCRATCH/gitleaks"
# The export takes its author from `git config` of this checkout; a machine without one still runs.
export GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=test
export GIT_CONFIG_KEY_1=user.email GIT_CONFIG_VALUE_1=test@example.invalid
printf '#!/bin/sh\nexit 0\n' > "$GITLEAKS"
chmod +x "$GITLEAKS"
git init -q --bare -b master "$RC_PUBLIC_WIKI_REMOTE"
remote_head() { git -C "$RC_PUBLIC_WIKI_REMOTE" rev-parse --verify --quiet master || true; }

# A wiki with one of everything: a public page with a private section, a private page, and a
# sidebar that lists both pages.
write_wiki() {
    rm -rf "$RC_WIKI"
    mkdir -p "$RC_WIKI/using"
    cat > "$RC_WIKI/home.md" <<'EOF'
# Home

See [downloads](using/downloads).
EOF
    cat > "$RC_WIKI/_sidebar.md" <<'EOF'
- [Home](home)
- [Downloads](using/downloads)
- [Maintainer notes](using/maintainer)
EOF
    cat > "$RC_WIKI/using/downloads.md" <<'EOF'
# Downloads

Public text.

<!-- private -->
## Release checklist

Only for the maintainer.
<!-- /private -->

## Queue

More public text.
EOF
    cat > "$RC_WIKI/using/maintainer.md" <<'EOF'
<!-- private page -->
# Maintainer notes

Secret process.
EOF
}
commit_wiki() {
    git -C "$RC_WIKI" init -q -b main 2> /dev/null || true
    git -C "$RC_WIKI" add -A
    git -C "$RC_WIKI" commit -q -m wiki
}
export_wiki() { run_status "$TOOLS/scripts/export-wiki.sh" 1.0.0; }
exported() { git -C "$RC_PUBLIC_WIKI_REMOTE" show "master:$1" 2> /dev/null; }

write_wiki
commit_wiki
rm -rf "$RC_PUBLIC_WIKI_DIR"
export_wiki
expect_status "a well-formed handbook is exported" 0
expect "without --push nothing is pushed" "" "$(remote_head)"
run_status "$TOOLS/scripts/export-wiki.sh" 1.0.0 --push
expect_status "--push publishes it" 0
expect "the public wiki carries the clone's commit" "$(git -C "$RC_PUBLIC_WIKI_DIR" rev-parse HEAD)" "$(remote_head)"
expect "the private page is left out" "" "$(exported using/maintainer.md)"
expect "and its sidebar line with it" "- [Home](Home)|- [Downloads](downloads)|" \
    "$(exported _Sidebar.md | tr '\n' '|')"
expect "the private section is cut, one blank line where it stood" \
    "# Downloads||Public text.||## Queue||More public text.|" "$(exported using/downloads.md | tr '\n' '|')"
expect_true "no marker text reaches the export" '! git -C "$RC_PUBLIC_WIKI_REMOTE" grep -q "private" master'
expect "the commit names the version" "Handbook for 1.0.0" "$(git -C "$RC_PUBLIC_WIKI_REMOTE" log -1 --format=%s master)"

refused() {
    local name="$1" needle="$2"
    commit_wiki
    local before; before="$(git -C "$RC_PUBLIC_WIKI_DIR" rev-parse HEAD)"
    export_wiki
    expect_status "$name: refused" 1
    expect_output "$name: named" "$needle"
    expect "$name: the clone is untouched" "$before" "$(git -C "$RC_PUBLIC_WIKI_DIR" rev-parse HEAD)"
    write_wiki
}

printf '\nAsk the [maintainer](maintainer).\n' >> "$RC_WIKI/home.md"
refused "a link to a private page" "link to the private page using/maintainer.md"

printf '\nSee [the checklist](using/downloads#release-checklist).\n' >> "$RC_WIKI/home.md"
refused "a link into a private section" "link into a private section"

printf '\n<!-- private -->\nnever closed\n' >> "$RC_WIKI/home.md"
refused "an unclosed section" "Home.md:5: private section never closed"

printf '\n<!-- private -->\n<!-- private -->\nx\n<!-- /private -->\n' >> "$RC_WIKI/home.md"
refused "a nested section" "private section opened inside the one from line 5"

printf '\n<!-- /private -->\n' >> "$RC_WIKI/home.md"
refused "a close without an open" "without an open private section"

printf '\n<!-- private page -->\n' >> "$RC_WIKI/home.md"
refused "a page marker below the first line" "counts only as the first line"

printf '\nWrite `<!-- private -->` to hide a section.\n' >> "$RC_WIKI/home.md"
refused "marker text inside a sentence" "marker text outside a marker line would be published"

printf '\nClone it from %s.\n' "$HOST" >> "$RC_WIKI/home.md"
refused "a private name" "wiki/Home.md:5: names something private"

# Built from pieces, so that this file does not link into docs/ itself: it is exported too.
REPO='https://github.com/degoya/rCommander'
printf '\nThe [design](%s/blob/main/docs/plugins.md).\n' "$REPO" >> "$RC_WIKI/home.md"
refused "a link into docs/ of the repository" "links to docs, which is not public"

mkdir -p "$RC_WIKI/develop"
printf '# Downloads for developers\n' > "$RC_WIKI/develop/downloads.md"
refused "two pages with one base name" "two pages share the base name 'downloads'"
rm -rf "$RC_WIKI/develop"

commit_wiki
before="$(git -C "$RC_PUBLIC_WIKI_DIR" rev-parse HEAD)"
run_status "$TOOLS/scripts/export-wiki.sh" 1.1.0-beta.1
expect_status "export-wiki: a beta is refused" 2
expect_output "saying why" "1.1.0-beta.1 is a pre-release: the public wiki stays on the last stable release"
expect "and the clone is untouched" "$before" "$(git -C "$RC_PUBLIC_WIKI_DIR" rev-parse HEAD)"
run_status "$TOOLS/scripts/export-wiki.sh" 1.1.0-rc.1
expect_status "export-wiki: another pre-release form is no version" 2

finish_tests export-wiki
