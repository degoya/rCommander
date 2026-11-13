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
#   rc_package_begin "$ZIP" "<stage>/VERSION.txt"       # both, and RC_BUILD_COMMIT/_TIME exported
#   list="$(rc_package_plugins "$ROOT" "$prebuilt")"   # the bundled .rcplug files, one per line
#   rc_package_plugin_list "$ROOT" "$prebuilt"         # ... into the array `plugins`
#   rc_package_payload "$STAGE" "$version" "$TARGET" "$profile"   # documents, plugins/, VERSION.txt
#   rc_package_version_txt "$ZIP" ["<stage>/VERSION.txt"]         # a package's VERSION.txt
#   rc_package_zip "$OUT" "<stage name>" "$ZIP"
#   rc_package_unportable "$STAGE"            # rc_package_zip runs it; the tarball calls it
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

# The text of VERSION.txt in the zip or tarball $1: the member $2 or, without it, the one
# `<folder>/VERSION.txt` at the archive's top folder. A failure, and nothing, when it cannot be
# read. The one reader of a package's VERSION.txt (RC-110-33): the guard below, the release
# chain's verify-artifacts and sign-update.sh.
rc_package_version_txt() {
    python3 -I - "$1" "${2:-}" << 'PY'
import sys
import tarfile
import zipfile

archive, member = sys.argv[1], sys.argv[2]
try:
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as package:
            names = package.namelist()
            read = package.read
            member = member or next(n for n in names if n.count("/") == 1 and n.endswith("/VERSION.txt"))
            text = read(member).decode("utf-8")
    else:
        with tarfile.open(archive) as package:
            names = package.getnames()
            member = member or next(n for n in names if n.count("/") == 1 and n.endswith("/VERSION.txt"))
            text = package.extractfile(member).read().decode("utf-8")
except (OSError, KeyError, AttributeError, StopIteration, UnicodeDecodeError, tarfile.TarError,
        zipfile.BadZipFile):
    sys.exit(1)
sys.stdout.write(text)
PY
}

# The `commit ...` line of member $2 (VERSION.txt) in the zip or tarball $1; a failure when it
# cannot be read.
rc_package_built_commit() {
    local text commit
    text="$(rc_package_version_txt "$1" "$2")" || return 1
    commit="$(sed -n 's/^commit //p' <<< "$text" | head -n 1)"
    [[ -n "$commit" ]] || return 1
    printf '%s\n' "$commit"
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

# What every package script does before it builds: the commit and the time, exported so the
# binary and VERSION.txt name the same ones (build.rs takes them verbatim), after the guard - an
# existing package $1 of this version from another build (its member $2) stops the run.
rc_package_begin() {
    local commit
    commit="$(rc_package_commit)"
    rc_package_guard "$1" "$2" "$commit" || return 1
    RC_BUILD_COMMIT="$commit"
    RC_BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    export RC_BUILD_COMMIT RC_BUILD_TIME
}

# rc_package_plugins into the array `plugins` of the caller; its failure is this one's.
rc_package_plugin_list() {
    local list package
    list="$(rc_package_plugins "$@")" || return $?
    plugins=()
    while IFS= read -r package; do [[ -z "$package" ]] || plugins+=("$package"); done <<< "$list"
}

# What every package carries beside the program, into the folder $1: README.md, LICENSE and
# NOTICE of the working directory, the plugins of the array `plugins` in plugins/, and
# VERSION.txt naming version $2, the build's commit and time (rc_package_begin), target $3 and
# profile $4.
rc_package_payload() {
    local stage="$1" version="$2" target="$3" profile="$4"
    install -m 644 README.md LICENSE NOTICE "$stage/" || return 1
    mkdir -p "$stage/plugins" || return 1
    if [[ "${#plugins[@]}" -gt 0 ]]; then
        install -m 644 "${plugins[@]}" "$stage/plugins/" || return 1
    fi
    printf '%s\n' "rCommander $version" "commit $RC_BUILD_COMMIT" "built $RC_BUILD_TIME" \
        "target $target, profile $profile" > "$stage/VERSION.txt"
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

# Fails, naming each, when the folder $1 carries what switches a copy to the portable mode
# (RC-100-95): the marker `rcommander.portable` anywhere, or a `data` beside a program
# (`rcommander`, `rcommander.exe`). A package never makes a copy portable by itself, and the
# update leaves both where they are, so one shipped would never go again.
rc_package_unportable() {
    local stage="$1" entry folder found=()
    while IFS= read -r -d '' entry; do
        folder="$(dirname "$entry")"
        case "$(basename "$entry")" in
            rcommander.portable) found+=("$entry") ;;
            data)
                if [[ -e "$folder/rcommander" || -e "$folder/rcommander.exe" ]]; then
                    found+=("$entry")
                fi
                ;;
        esac
    done < <(find "$stage" \( -name rcommander.portable -o -name data \) -print0)
    if ((${#found[@]})); then
        printf '!! the package carries what makes a copy portable: %s\n' "${found[@]}" >&2
        echo "   No package carries rcommander.portable or data beside the program; nothing was packaged." >&2
        return 1
    fi
}

# Zips the folder $2 of directory $1 into $3 (an absolute path), the folder at the top: with
# `zip` where it is installed, else with Python's zipfile, which keeps the files' modes - the
# Windows runner's Git Bash has no `zip` (RC-100-31). A folder that carries the portable mode's
# marker or data is refused first (rc_package_unportable).
rc_package_zip() {
    local dir="$1" name="$2" zip="$3" python
    rc_package_unportable "$dir/$name" || return 1
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
