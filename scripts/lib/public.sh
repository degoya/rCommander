# shellcheck shell=bash
# Shared by export-public.sh and export-wiki.sh (RC-100-30): the version forms, the secret
# scanner, the exclude list and the link and name checks against it, the identity that authors a
# public commit, and the local clone whose tree an export replaces. Copied and adapted from
# rDownloader's scripts/lib/public.sh at commit 26c8592a6.
#
# Sourced, never run, after ROOT is set. Every function exits the calling script on a refusal,
# with the reason.

# Whether $1 is a version a release carries: X.Y.Z, or X.Y.Z-beta.N for a pre-release.
rc_public_version() {
    [[ "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-beta\.[1-9][0-9]*)?$ ]]
}

# Whether version $1 is a pre-release.
rc_public_prerelease() {
    [[ "$1" == *-beta.* ]]
}

# Sets GITLEAKS_BIN: $GITLEAKS, else `gitleaks` on PATH, and nothing else. A fixed fallback path
# would let whatever binary sat there decide whether secrets reach the public export. A GITLEAKS
# that names no executable is refused rather than replaced by the one on PATH.
rc_public_find_gitleaks() {
    GITLEAKS_BIN="${GITLEAKS:-}"
    if [[ -n "$GITLEAKS_BIN" && ! -x "$GITLEAKS_BIN" ]]; then
        echo "GITLEAKS=$GITLEAKS_BIN is not an executable file." >&2
        echo "Nothing is exported without the secret scan." >&2
        exit 1
    fi
    if [[ -z "$GITLEAKS_BIN" ]]; then
        GITLEAKS_BIN="$(command -v gitleaks || true)"
    fi
    if [[ -z "$GITLEAKS_BIN" ]]; then
        echo "gitleaks not found: put it on PATH or name it with GITLEAKS=<path>." >&2
        echo "Nothing is exported without the secret scan. Releases: https://github.com/gitleaks/gitleaks/releases" >&2
        exit 1
    fi
}

# Sets AUTHOR_NAME and AUTHOR_EMAIL from the git identity of the repository in $1.
rc_public_find_author() {
    AUTHOR_NAME="$(git -C "$1" config user.name || true)"
    AUTHOR_EMAIL="$(git -C "$1" config user.email || true)"
    if [[ -z "$AUTHOR_NAME" || -z "$AUTHOR_EMAIL" ]]; then
        echo "git user.name and user.email must be set in $1; they author the export" >&2
        exit 1
    fi
}

# Scans directory $1 from inside it, so that a .gitleaks.toml at its root applies.
rc_public_scan() {
    echo "==> gitleaks on the export"
    if ! (cd "$1" && "$GITLEAKS_BIN" dir . --no-banner --redact --exit-code 1); then
        echo "!! gitleaks found something in the export; nothing was committed." >&2
        echo "   A real secret is removed at the source. A fixture gets an entry in .gitleaks.toml." >&2
        exit 1
    fi
}

# Removes from tree $1 every path the exclude list $2 names (its header states the format); a
# path the tree does not have is named, not an error.
rc_public_exclude() {
    local stage="$1" list="$2" line path excluded=0
    while IFS= read -r line || [[ -n "$line" ]]; do
        path="${line%%#*}"
        path="${path#"${path%%[![:space:]]*}"}"
        path="${path%"${path##*[![:space:]]}"}"
        [[ -n "$path" ]] || continue
        case "$path" in
            /*|*..*) echo "refusing exclude entry outside the tree: $path" >&2; exit 1 ;;
        esac
        path="${path%/}"
        if [[ -e "$stage/$path" || -L "$stage/$path" ]]; then
            rm -rf -- "${stage:?}/$path"
            excluded=$((excluded + 1))
        else
            echo "    (not in this tree: $path)"
        fi
    done < "$list"
    echo "    left out $excluded path(s) named in scripts/public-exclude.txt"
}

# Refuses a link in tree $1 into a path the exclude list $2 names: it would be dead in the
# public repository. $3 is `--wiki` for a converted wiki tree, whose relative links name wiki
# pages rather than repository paths. The rules are in lib/public-links.py.
rc_public_check_links() {
    local findings
    echo "==> links into paths the public repository leaves out"
    if ! findings="$(python3 -B "$ROOT/scripts/lib/public-links.py" "$@")"; then
        echo "!! the export links into what it leaves out; nothing was committed:" >&2
        printf '%s\n' "$findings" | sed 's/^/   /' >&2
        echo "   Point the link at the public wiki (https://github.com/degoya/rCommander/wiki) or drop it." >&2
        exit 1
    fi
    echo "    none"
}

# Refuses a private name - a private host, a home directory, a private repository, a private
# address - in tree $1 outside scripts/public-names-allow.txt. $2 is `--wiki` for a converted
# wiki tree. The names themselves are in scripts/public-names.txt, which the export leaves out;
# the rules are in lib/public-names.py.
rc_public_check_names() {
    local tree="$1" findings
    shift
    echo "==> private names"
    if [[ ! -f "$ROOT/scripts/public-names.txt" ]]; then
        echo "!! scripts/public-names.txt is missing: an export runs from the development repository" >&2
        exit 1
    fi
    if ! findings="$(python3 -B "$ROOT/scripts/lib/public-names.py" "$tree" \
        --terms "$ROOT/scripts/public-names.txt" --allow "$ROOT/scripts/public-names-allow.txt" "$@")"; then
        echo "!! the export names something private; nothing was committed:" >&2
        printf '%s\n' "$findings" | sed 's/^/   /' >&2
        echo "   Remove it at the source, or allow a placeholder in scripts/public-names-allow.txt." >&2
        exit 1
    fi
    echo "    none"
}

# Brings the clone $1 of $2 to the tip of branch $3, cloning it when missing. An empty remote
# - the very first export - leaves HEAD unborn on $3. `pub` runs git in it from then on.
rc_public_prepare_clone() {
    local dir="$1" remote="$2" branch="$3"
    RC_PUBLIC_CLONE="$dir"
    if [[ ! -d "$dir/.git" ]]; then
        [[ ! -e "$dir" ]] || { echo "$dir exists but is not a git clone" >&2; exit 1; }
        echo "==> cloning $remote into $dir"
        git clone "$remote" "$dir"
    fi
    if [[ -n "$(git -C "$dir" status --porcelain)" ]]; then
        echo "$dir has uncommitted changes; clean it up first - the export replaces its tree" >&2
        exit 1
    fi
    git -C "$dir" fetch --quiet --tags origin || { echo "cannot fetch $remote" >&2; exit 1; }

    if git -C "$dir" rev-parse --verify --quiet "refs/heads/$branch" > /dev/null; then
        git -C "$dir" checkout --quiet "$branch"
        if git -C "$dir" rev-parse --verify --quiet "refs/remotes/origin/$branch" > /dev/null; then
            git -C "$dir" merge --quiet --ff-only "origin/$branch" \
                || { echo "local $branch has diverged from origin/$branch" >&2; exit 1; }
        fi
    elif git -C "$dir" rev-parse --verify --quiet "refs/remotes/origin/$branch" > /dev/null; then
        git -C "$dir" checkout --quiet -b "$branch" "origin/$branch"
    else
        echo "    the repository has no $branch yet; this is its first commit"
        git -C "$dir" symbolic-ref HEAD "refs/heads/$branch"
    fi
}

# git in the clone rc_public_prepare_clone prepared: `pub commit ...`.
pub() { git -C "${RC_PUBLIC_CLONE:?rc_public_prepare_clone first}" "$@"; }

# Replaces everything in clone $1 but .git with the tree in $2 and stages the result: a file the
# export no longer has is deleted, not left behind.
rc_public_replace_tree() {
    echo "==> replacing the tree of $1"
    find "$1" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf -- {} +
    cp -a "$2"/. "$1"/
    git -C "$1" add --all
}
