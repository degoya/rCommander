# shellcheck shell=bash
# Shared by export-public.sh and export-wiki.sh (RC-100-30): the version forms, the secret
# scanner, the exclude list and the link and name checks against it, the identity that authors a
# public commit, and the local clone whose tree an export replaces - locked for one export at a
# time, its push held to the tree that was staged (RC-110-31). Copied and adapted from
# rDownloader's scripts/lib/public.sh at commit 26c8592a6.
#
# Sourced, never run, after ROOT is set. Every function exits the calling script on a refusal,
# with the reason.
#
# Environment:
#   RC_PUBLIC_LOCK_WAIT  seconds an export waits for another one working in the same clone
#                        (default 600), then refuses
#   RC_PUBLIC_FETCH_PAUSE  seconds between the three tries of the clone's fetch (default 10)
#   RC_PUBLIC_LISTS      the directory of public-names.txt and public-names-allow.txt (default
#                        $ROOT/scripts; export-public.sh sets it to the exported commit's lists)

# The version forms an export takes are the release's: rc_release_version, rc_is_prerelease.
# shellcheck source=release-tag.sh
source "$(dirname "${BASH_SOURCE[0]}")/release-tag.sh"

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
    local tree="$1" findings lists="${RC_PUBLIC_LISTS:-$ROOT/scripts}"
    shift
    echo "==> private names"
    if [[ ! -f "$lists/public-names.txt" ]]; then
        echo "!! scripts/public-names.txt is missing: an export runs from the development repository" >&2
        exit 1
    fi
    if ! findings="$(python3 -B "$ROOT/scripts/lib/public-names.py" "$tree" \
        --terms "$lists/public-names.txt" --allow "$lists/public-names-allow.txt" "$@")"; then
        echo "!! the export names something private; nothing was committed:" >&2
        printf '%s\n' "$findings" | sed 's/^/   /' >&2
        echo "   Remove it at the source, or allow a placeholder in scripts/public-names-allow.txt." >&2
        exit 1
    fi
    echo "    none"
}

# One export at a time per clone (RC-110-31): a lock on a file in the clone's .git, taken before
# anything in the clone is read or replaced and held until the calling script ends - past its
# commit and its push. Two exports through one clone at once - the release's and a public CI
# run's - would otherwise delete and copy the same files and could commit a mix of two trees.
# This is not the build lock (lib/lock.sh): it serialises nothing but this clone. Without `flock`
# (macOS) it says so and goes on.
rc_public_lock_clone() {
    local dir="$1" file wait="${RC_PUBLIC_LOCK_WAIT:-600}"
    RC_PUBLIC_LOCK_FD=""
    [[ "$wait" =~ ^(0|[1-9][0-9]*)$ ]] \
        || { echo "RC_PUBLIC_LOCK_WAIT must be a whole number of seconds, not '$wait'" >&2; exit 2; }
    if ! command -v flock > /dev/null 2>&1; then
        echo "note: flock is not installed, so exports into $dir are not serialised" >&2
        return 0
    fi
    file="$(git -C "$dir" rev-parse --absolute-git-dir)/rc-export.lock"
    exec {RC_PUBLIC_LOCK_FD}>> "$file"
    flock -n "$RC_PUBLIC_LOCK_FD" && return 0
    echo "==> waiting for $file (another export works in $dir)"
    if ! flock -w "$wait" "$RC_PUBLIC_LOCK_FD"; then
        echo "!! gave up after ${wait}s waiting for $file; nothing was exported." >&2
        echo "   Another export into $dir is still running (fuser -v $file)." >&2
        exit 1
    fi
}

# Fetches the origin of clone $1 with its tags, up to three times (RC-110-33): a single failed
# fetch - the network or GitHub for a moment - stopped the releases 0.9.0 and 0.18.0 here.
rc_public_fetch() {
    local dir="$1" attempt pause="${RC_PUBLIC_FETCH_PAUSE:-10}"
    for attempt in 1 2 3; do
        git -C "$dir" fetch --quiet --tags origin && return 0
        [[ "$attempt" -lt 3 ]] || break
        echo "    fetch attempt $attempt of 3 failed; again in ${pause} s" >&2
        sleep "$pause"
    done
    return 1
}

# Brings the clone $1 of $2 to the tip of branch $3, cloning it when missing, and locks it for
# this script (rc_public_lock_clone). An empty remote - the very first export - leaves HEAD
# unborn on $3. `pub` runs git in it from then on.
rc_public_prepare_clone() {
    local dir="$1" remote="$2" branch="$3"
    RC_PUBLIC_CLONE="$dir"
    if [[ ! -d "$dir/.git" ]]; then
        [[ ! -e "$dir" ]] || { echo "$dir exists but is not a git clone" >&2; exit 1; }
        echo "==> cloning $remote into $dir"
        git clone "$remote" "$dir"
    fi
    rc_public_lock_clone "$dir"
    if [[ -n "$(git -C "$dir" status --porcelain)" ]]; then
        echo "$dir has uncommitted changes; clean it up first - the export replaces its tree" >&2
        exit 1
    fi
    rc_public_fetch "$dir" || { echo "cannot fetch $remote (three tries)" >&2; exit 1; }

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

# git in the clone rc_public_prepare_clone prepared: `pub commit ...`. Without the lock's
# descriptor: a helper git leaves behind (`gc --auto` after a commit) would hold the lock on.
pub() {
    if [[ -n "${RC_PUBLIC_LOCK_FD:-}" ]]; then
        git -C "${RC_PUBLIC_CLONE:?rc_public_prepare_clone first}" "$@" {RC_PUBLIC_LOCK_FD}>&-
    else
        git -C "${RC_PUBLIC_CLONE:?rc_public_prepare_clone first}" "$@"
    fi
}

# Replaces everything in clone $1 but .git with the tree in $2 and stages the result: a file the
# export no longer has is deleted, not left behind. RC_PUBLIC_STAGED_TREE is the tree staged,
# which rc_public_verify_push holds a push to.
rc_public_replace_tree() {
    echo "==> replacing the tree of $1"
    find "$1" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf -- {} +
    cp -a "$2"/. "$1"/
    git -C "$1" add --all
    RC_PUBLIC_STAGED_TREE="$(git -C "$1" write-tree)"
}

# Before a push of ref $1 from the prepared clone: its tree is the one this export staged, and the
# clone's files, staged anew, are still HEAD's tree - nothing came into the clone between the
# commit and the push. Otherwise nothing is pushed.
rc_public_verify_push() {
    local ref="$1" committed index head
    committed="$(pub rev-parse --verify --quiet "$ref^{tree}" || true)"
    head="$(pub rev-parse --verify --quiet 'HEAD^{tree}' || true)"
    pub add --all
    index="$(pub write-tree)"
    if [[ -z "$committed" || "$committed" != "${RC_PUBLIC_STAGED_TREE:-unset}" || "$index" != "$head" ]]; then
        echo "!! refusing to push $ref from $RC_PUBLIC_CLONE: nothing was pushed." >&2
        echo "   staged by this export: ${RC_PUBLIC_STAGED_TREE:-nothing}" >&2
        echo "   committed as $ref:      ${committed:-nothing}" >&2
        echo "   the clone's files now:  $index (HEAD: ${head:-unborn})" >&2
        echo "   Something else wrote into the clone; look at it (git -C $RC_PUBLIC_CLONE status)." >&2
        exit 1
    fi
}

# Whether tag $1 of the prepared clone on branch $2 was only ever staged here: it sits on HEAD,
# the remote has no such tag, and the remote's branch does not contain HEAD. Such a commit the
# release chain staged before its tag and may replace; anything the remote has is never rewritten.
rc_public_unpublished() {
    local tag="$1" branch="$2" status=0
    [[ "$(pub rev-parse --verify --quiet "$tag^{commit}")" == "$(pub rev-parse --verify --quiet HEAD)" ]] || return 1
    pub ls-remote --exit-code --tags origin "refs/tags/$tag" > /dev/null 2>&1 || status=$?
    [[ "$status" -eq 2 ]] || return 1
    ! pub rev-parse --verify --quiet "refs/remotes/origin/$branch" > /dev/null \
        || ! pub merge-base --is-ancestor HEAD "refs/remotes/origin/$branch"
}
