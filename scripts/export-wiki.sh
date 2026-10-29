#!/usr/bin/env bash
#
# Exports the user handbook to the public repository's GitHub wiki (RC-100-30). Copied and adapted
# from rDownloader's scripts/export-wiki.sh at commit 26c8592a6.
#
# The handbook is written in the private wiki (~/projects/rcommander.wiki, GitLab form: page
# folders, `home.md`, `_sidebar.md`, links with folder paths). The public one is
# github.com/degoya/rCommander/wiki, whose git repository is rCommander.wiki.git. This script
# takes `git archive HEAD` of the private wiki - its committed state, never an unsaved edit -
# converts it to GitHub-wiki form, refuses a private name (scripts/public-names.txt) outside the
# `wiki/` entries of scripts/public-names-allow.txt, warns about every remaining mention of
# `docs/`, scans it with gitleaks and replaces the tree of a local clone
# of the public wiki with it, keeping that clone's `.git`. The commit is "Handbook for
# <version>", a snapshot: the private wiki's history does not come along.
#
# The conversion:
#   * home.md becomes Home.md and _sidebar.md _Sidebar.md (at the root; GitHub's names).
#   * A page link loses its folder path: `(getting-started/installation)` and
#     `(../using/x.md#a)` become `(installation)` and `(x#a)`. GitHub resolves a page by its
#     base name in any folder, so the folders stay - and two pages sharing a base name are
#     refused, because one of them could never be linked.
#   * An image or other file link becomes relative to the wiki root: `../images/a.png` and
#     `images/a.png` both become `images/a.png`, which GitHub serves for every page.
#   * A link to a page or file that does not exist is refused rather than exported dead, and so
#     is a link to the source repository at a path scripts/public-exclude.txt leaves out - all of
#     docs/ among them.
#   * The GitLab-only `.gitlab/` folder is left out.
#
# Private material stays in the one source, marked, and never leaves it:
#   * A page whose first line is `<!-- private page -->` is left out, and so is every list item
#     of `_sidebar.md` / `_footer.md` that links to it.
#   * The lines from a `<!-- private -->` line to the next `<!-- /private -->` line, both
#     included, are removed from the page. Each marker is a line of its own.
#   * Refused: a public page linking to a private page or to a heading inside a removed section,
#     an unclosed, nested or unopened marker, a page marker below the first line, and any marker
#     text left in the converted tree.
#
# Nothing leaves this machine without --push, and every push is the owner's word. A pre-release
# (`X.Y.Z-beta.N`) is refused: the public handbook describes the stable release and follows the
# next one.
#
# Usage:
#   scripts/export-wiki.sh 0.9.0           # convert, commit into the local clone
#   scripts/export-wiki.sh 0.9.0 --push    # ... and push it
#
# Environment:
#   RC_WIKI                  the private wiki (default: rcommander.wiki beside the main checkout,
#                            found from a worktree too, as scripts/keyboard-reference.sh does)
#   RC_PUBLIC_WIKI_DIR       the local clone (default: ~/projects/rCommander-public.wiki)
#   RC_PUBLIC_WIKI_REMOTE    where to clone it from (default: git@github.com:degoya/rCommander.wiki.git)
#   RC_PUBLIC_WIKI_BRANCH    the branch GitHub renders (default: master)
#   GITLEAKS                 the gitleaks binary, as for export-public.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/public.sh
source "$ROOT/scripts/lib/public.sh"
cd "$ROOT"

if [[ -z "${RC_WIKI:-}" ]]; then
    # The main checkout, also from a worktree: the parent of the common git directory.
    common="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2> /dev/null || echo "$ROOT/.git")"
    RC_WIKI="$(dirname "$(dirname "$common")")/rcommander.wiki"
fi
WIKI_SRC="$RC_WIKI"
WIKI_DIR="${RC_PUBLIC_WIKI_DIR:-$HOME/projects/rCommander-public.wiki}"
WIKI_REMOTE="${RC_PUBLIC_WIKI_REMOTE:-git@github.com:degoya/rCommander.wiki.git}"
WIKI_BRANCH="${RC_PUBLIC_WIKI_BRANCH:-master}"

VERSION=""
DO_PUSH=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --push) DO_PUSH=1; shift ;;
        -h|--help) sed -n '2,53p' "$0"; exit 0 ;;
        -*) echo "unknown argument: $1" >&2; exit 2 ;;
        *)
            [[ -z "$VERSION" ]] || { echo "unexpected argument: $1" >&2; exit 2; }
            VERSION="$1"; shift ;;
    esac
done
if ! rc_public_version "$VERSION"; then
    echo "usage: scripts/export-wiki.sh <version> [--push]" >&2
    exit 2
fi
if rc_public_prerelease "$VERSION"; then
    echo "$VERSION is a pre-release: the public wiki stays on the last stable release until the next one" >&2
    exit 2
fi

SOURCE_COMMIT="$(git -C "$WIKI_SRC" rev-parse --verify --quiet HEAD)" \
    || { echo "$WIKI_SRC is not a git checkout with a commit" >&2; exit 1; }
rc_public_find_gitleaks
rc_public_find_author "$ROOT"

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/rc-wiki-export.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT

echo "==> exporting the handbook at ${SOURCE_COMMIT:0:12} to a staging tree"
git -C "$WIKI_SRC" archive --format=tar HEAD | tar -x -C "$STAGE"
rm -rf "${STAGE:?}/.gitlab"

echo "==> converting to GitHub-wiki form"
python3 - "$STAGE" <<'PY'
import os
import posixpath
import re
import sys

root = sys.argv[1]
RENAMES = {"home.md": "Home.md", "_sidebar.md": "_Sidebar.md", "_footer.md": "_Footer.md"}

for old, new in RENAMES.items():
    if os.path.exists(os.path.join(root, old)):
        os.rename(os.path.join(root, old), os.path.join(root, new))

problems = []
LINK = re.compile(r'(!?\[[^\]]*\])\(([^)\s]+)((?:\s+"[^"]*")?)\)')
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
PAGE_MARK, OPEN, CLOSE = "<!-- private page -->", "<!-- private -->", "<!-- /private -->"
MARKER_TEXT = re.compile(r"<!--\s*/?\s*private\b")
HEADING = re.compile(r"^#{1,6}\s+(.*?)\s*#*\s*$")
LIST_ITEM = re.compile(r"^\s*[-*+]\s")


def markdown_files():
    for directory, _, files in os.walk(root):
        for name in sorted(files):
            if name.endswith(".md"):
                full = os.path.join(directory, name)
                yield posixpath.relpath(full, root).replace(os.sep, "/"), full


def stem_of(path):
    return posixpath.splitext(posixpath.basename(path))[0].lower()


def anchor(heading):
    # GitHub's heading id: lower case, punctuation dropped, spaces become hyphens.
    return re.sub(r"[^\w\- ]", "", heading.strip().lower()).replace(" ", "-")


# Private pages go first, so nothing below can see them as pages.
private_pages = {}  # lower-case base name -> path relative to the root
for relative, full in list(markdown_files()):
    with open(full, encoding="utf-8") as handle:
        if handle.readline().strip() == PAGE_MARK:
            private_pages[stem_of(relative)] = relative
            os.remove(full)


def private_page(target):
    if target.startswith("#") or SCHEME.match(target):
        return None
    path = target.partition("#")[0]
    if posixpath.splitext(path)[1] not in ("", ".md"):
        return None
    return private_pages.get(stem_of(path))


# Then the private sections, remembering the heading ids that went with them.
removed_anchors = {}  # lower-case base name -> ids no longer on the page
sections = 0
for relative, full in markdown_files():
    with open(full, encoding="utf-8") as handle:
        lines = handle.read().split("\n")
    key = stem_of(relative)
    out, opened, joined, fenced, kept, gone = [], 0, False, False, set(), set()
    for number, line in enumerate(lines, 1):
        text = line.strip()
        if text == PAGE_MARK:
            problems.append(f"{relative}:{number}: {PAGE_MARK} counts only as the first line")
            continue
        if text == OPEN:
            if opened:
                problems.append(f"{relative}:{number}: private section opened inside the one from line {opened}")
            else:
                opened = number
            continue
        if text == CLOSE:
            if not opened:
                problems.append(f"{relative}:{number}: {CLOSE} without an open private section")
            else:
                opened, joined, sections = 0, True, sections + 1
            continue
        if text.startswith(("```", "~~~")):
            fenced = not fenced
        heading = None if fenced else HEADING.match(line)
        if opened:
            if heading:
                gone.add(anchor(heading.group(1)))
            continue
        if heading:
            kept.add(anchor(heading.group(1)))
        if MARKER_TEXT.search(line):
            # The markers never reach the public wiki, not even quoted in a sentence.
            problems.append(f"{relative}:{number}: marker text outside a marker line would be published")
        if key in ("_sidebar", "_footer") and LIST_ITEM.match(line) \
                and any(private_page(m.group(2)) for m in LINK.finditer(line)):
            continue
        # One blank line where a section stood, not two.
        if joined and not text and out and not out[-1].strip():
            continue
        joined = False
        out.append(line)
    if opened:
        problems.append(f"{relative}:{opened}: private section never closed")
    if joined or len(out) < len(lines):
        # A section at the end would otherwise leave the page without its final newline.
        while out and not out[-1].strip():
            out.pop()
        out.append("")
    removed_anchors[key] = gone - kept
    with open(full, "w", encoding="utf-8") as handle:
        handle.write("\n".join(out))

pages = {}  # lower-case base name -> path relative to the root
for relative, _ in markdown_files():
    key = stem_of(relative)
    if key in pages:
        problems.append(f"two pages share the base name {posixpath.basename(relative)[:-3]!r}: {pages[key]} and {relative}")
    pages[key] = relative


def refuse_private_anchor(page, key, fragment, target):
    if fragment and fragment.lower() in removed_anchors.get(key, ()):
        problems.append(f"{page}: link into a private section: {target}")


def convert(page, target):
    if SCHEME.match(target):
        return target
    path, _, fragment = target.partition("#")
    if not path:
        refuse_private_anchor(page, stem_of(page), fragment, target)
        return target
    hidden = private_page(target)
    if hidden:
        problems.append(f"{page}: link to the private page {hidden}: {target}")
        return target
    base = posixpath.basename(path)
    stem, extension = posixpath.splitext(base)
    if extension in ("", ".md"):
        found = pages.get(stem.lower())
        if found is None:
            problems.append(f"{page}: link to a page that does not exist: {target}")
            return target
        refuse_private_anchor(page, stem.lower(), fragment, target)
        return posixpath.basename(found)[:-3] + ("#" + fragment if fragment else "")
    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(page), path))
    if resolved.startswith("../") or not os.path.isfile(os.path.join(root, resolved)):
        problems.append(f"{page}: link to a file that does not exist: {target}")
        return target
    return resolved + ("#" + fragment if fragment else "")


for page in sorted(pages.values()):
    full = os.path.join(root, page)
    with open(full, encoding="utf-8") as handle:
        lines = handle.read().split("\n")
    # Links are rewritten per run of prose between code fences, not per line: a link whose
    # text wraps (`[the diagnostics\nlog](../operating/...)`) was missed line by line and reached
    # GitHub as a dead folder link (first real export, 2026-09-25).
    out, prose, fenced = [], [], False

    def flush():
        if prose:
            out.append(LINK.sub(
                lambda m: f"{m.group(1)}({convert(page, m.group(2))}{m.group(3)})",
                "\n".join(prose),
            ))
            prose.clear()

    for line in lines:
        fence = line.lstrip().startswith(("```", "~~~"))
        if fenced or fence:
            flush()
            out.append(line)
            if fence:
                fenced = not fenced
        else:
            prose.append(line)
    flush()
    with open(full, "w", encoding="utf-8") as handle:
        handle.write("\n".join(out))

if problems:
    print("!! the handbook cannot be exported as it is:", file=sys.stderr)
    for problem in problems:
        print("   " + problem, file=sys.stderr)
    sys.exit(1)
print(f"    {len(pages)} pages converted; left out: {len(private_pages)} private pages, "
      f"{sections} private sections")
PY

# The handbook is public; the developer documentation under docs/ and whatever else
# scripts/public-exclude.txt names is not, so a link to it would be dead on GitHub.
rc_public_check_links "$STAGE" "$ROOT/scripts/public-exclude.txt" --wiki
rc_public_check_names "$STAGE" --wiki
rc_public_scan "$STAGE"

# A path into docs/ named in prose is no link, but the reader cannot follow it either.
echo "==> mentions of docs/ that remain (a warning, not a refusal)"
warnings="$(cd "$STAGE" && { grep -rnE '(^|[^[:alnum:]_/.-])docs/' --include='*.md' . || true; })"
if [[ -n "$warnings" ]]; then
    echo "WARNING: $(wc -l <<< "$warnings") line(s) of the handbook name docs/:"
    while IFS= read -r line; do echo "    $line"; done <<< "$warnings"
else
    echo "    none"
fi

rc_public_prepare_clone "$WIKI_DIR" "$WIKI_REMOTE" "$WIKI_BRANCH"
rc_public_replace_tree "$WIKI_DIR" "$STAGE"

if pub rev-parse --verify --quiet HEAD > /dev/null && pub diff --cached --quiet HEAD; then
    echo "==> the public wiki already carries exactly this handbook"
else
    pub -c user.name="$AUTHOR_NAME" -c user.email="$AUTHOR_EMAIL" commit --quiet \
        -m "Handbook for $VERSION"
    echo "==> committed $(pub rev-parse --short HEAD) on $WIKI_BRANCH"
fi

if [[ "$DO_PUSH" -eq 1 ]]; then
    pub push origin "$WIKI_BRANCH"
    echo "==> pushed $WIKI_BRANCH to $WIKI_REMOTE"
else
    echo "==> not pushed. Publish with: git -C $WIKI_DIR push origin $WIKI_BRANCH"
fi
