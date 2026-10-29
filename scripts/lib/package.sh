# shellcheck shell=bash
#
# What the package scripts share (RC-080-41): the version a package is named by, the commit it is
# built from, and the owner's rule that one version never names two builds (AGENTS.md, "A new
# package with a new feature is at least a new minor version").
#
#   source "$ROOT/scripts/lib/package.sh"
#   version="$(rc_package_version "$ROOT")"   # fails on an empty or inconsistent version
#   commit="$(rc_package_commit)"              # 1a2b3c4d, 1a2b3c4d-dirty or unknown
#   rc_package_guard "$ZIP" "<stage>/VERSION.txt" "$commit"
#   list="$(rc_package_plugins "$ROOT" "$prebuilt")"   # the bundled .rcplug files, one per line
#   rc_package_zip "$OUT" "<stage name>" "$ZIP"
#
# rc_package_guard fails when the package already exists and was built from another commit, from
# a dirty tree, or names no commit at all - then the version is raised (scripts/set-version.sh).
# RC_REPACK=1 replaces it anyway.

# shellcheck source=workspace-version.sh
source "$(dirname "${BASH_SOURCE[0]}")/workspace-version.sh"

# The workspace version of the checkout $1, after scripts/set-version.sh --check held it; nothing
# and a failure when there is none or it is inconsistent, so no package is named `rcommander--...`.
rc_package_version() {
    local root="$1" version
    version="$(rc_workspace_version < "$root/Cargo.toml")"
    if [[ -z "$version" ]]; then
        echo "!! Cargo.toml has no [workspace.package] version; nothing was packaged" >&2
        return 1
    fi
    if ! "$root/scripts/set-version.sh" --check >&2; then
        echo "!! scripts/set-version.sh --check failed; nothing was packaged" >&2
        return 1
    fi
    printf '%s\n' "$version"
}

# The commit of the working tree, `-dirty` when anything differs from it - untracked files
# included, ignored ones not.
rc_package_commit() {
    local commit
    commit="$(git rev-parse --short=8 HEAD 2> /dev/null)" || { echo unknown; return 0; }
    [[ -z "$(git status --porcelain 2> /dev/null)" ]] || commit="$commit-dirty"
    printf '%s\n' "$commit"
}

# The `commit ...` line of member $2 (VERSION.txt) in the zip or tarball $1; a failure when it
# cannot be read.
rc_package_built_commit() {
    python3 -I - "$1" "$2" << 'PY'
import sys
import tarfile
import zipfile

archive, member = sys.argv[1], sys.argv[2]
try:
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as package:
            text = package.read(member).decode("utf-8")
    else:
        with tarfile.open(archive) as package:
            text = package.extractfile(member).read().decode("utf-8")
except (OSError, KeyError, AttributeError, UnicodeDecodeError, tarfile.TarError, zipfile.BadZipFile):
    sys.exit(1)
for line in text.splitlines():
    if line.startswith("commit "):
        print(line[len("commit "):])
        sys.exit(0)
sys.exit(1)
PY
}

# Whether the package $1 may be written for commit $3: it does not exist yet, or its member $2
# names the same clean commit, or RC_REPACK=1.
rc_package_guard() {
    local archive="$1" member="$2" commit="$3" found
    [[ -e "$archive" ]] || return 0
    if [[ "${RC_REPACK:-0}" == 1 ]]; then
        echo "note: RC_REPACK=1, so $archive is replaced whatever build it holds" >&2
        return 0
    fi
    found="$(rc_package_built_commit "$archive" "$member")" || found=""
    if [[ -n "$found" && "$found" == "$commit" && "$commit" != *-dirty && "$commit" != unknown ]]; then
        return 0
    fi
    echo "!! $archive exists, built from commit ${found:-(not readable)}; this build is $commit." >&2
    echo "   One version never names two builds: raise it with scripts/set-version.sh, or" >&2
    echo "   RC_REPACK=1 replaces the package. Nothing was built." >&2
    return 1
}

# The bundled plugin packages of checkout $1, one path per line: built and signed by
# scripts/build-plugins.sh, or - with a directory as $2, a CI job's signed packages (RC-100-31) -
# taken from there unbuilt. A plugin without its package is a failure. Every command checks its
# own status: a command substitution runs without `set -e`.
rc_package_plugins() {
    local root="$1" from="${2:-}" names name version package
    if [[ -z "$from" ]]; then
        echo "==> building the bundled plugins" >&2
        "$root/scripts/build-plugins.sh" >&2 || return $?
        from="$root/artifacts/plugins"
    fi
    names="$("$root/scripts/build-plugins.sh" --list)" || return $?
    while IFS= read -r name; do
        [[ -n "$name" ]] || continue
        version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/plugins/$name/manifest.toml" | head -n 1)"
        package="$from/$name-$version.rcplug"
        [[ -f "$package" ]] || { echo "!! no plugin package $package" >&2; return 1; }
        printf '%s\n' "$package"
    done <<< "$names"
}

# Zips the folder $2 of directory $1 into $3 (an absolute path), the folder at the top: with
# `zip` where it is installed, else with Python's zipfile, which keeps the files' modes - the
# Windows runner's Git Bash has no `zip` (RC-100-31).
rc_package_zip() {
    local dir="$1" name="$2" zip="$3" python
    if command -v zip > /dev/null 2>&1; then
        (cd "$dir" && zip -qry "$zip" "$name")
        return
    fi
    python="$(command -v python3 || command -v python)" || { echo "!! neither zip nor python" >&2; return 1; }
    "$python" -I - "$dir" "$name" "$zip" << 'PY'
import os
import sys
import zipfile

base, name, target = sys.argv[1], sys.argv[2], sys.argv[3]
with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as package:
    for folder, dirs, files in os.walk(os.path.join(base, name)):
        dirs.sort()
        for entry in sorted(dirs) + sorted(files):
            path = os.path.join(folder, entry)
            package.write(path, os.path.relpath(path, base).replace(os.sep, "/"))
PY
}
