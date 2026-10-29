#!/usr/bin/env bash
#
# The screenshots of the website and the wiki, taken on Windows from WSL (RC-100-45): builds and
# starts rcommander.exe through scripts/run-windows.sh with the hidden flag
# `--screenshots <scenes.json> --out <dir> --demo-root <dir>`, waits for the run's
# screenshots.json, and fetches the pictures to artifacts/screenshots/<version>/. The window runs
# on a throwaway profile and a demo tree; it never sees the person's database or files.
#
# Usage:
#   scripts/screenshots.sh [--release] [--deliver] [--timeout <seconds>]
#
#   --release   a release build (default: debug)
#   --deliver   also copy the pictures to the website (public/screenshots/) and the wiki
#               (images/screenshots/)
#   --timeout   how long to wait for the run (default 600 s)
#
# The scenes are scripts/screenshots/scenes.json, their alt texts scripts/screenshots/alts.json.
# Environment: RC_SHOTS_DEMO_ROOT (default C:\rCommander-Demo, a folder that must not exist or
# be empty; the run removes it), RC_WEBSITE_DIR (default ../rcommander-website), RC_WIKI_DIR
# (default ../rcommander.wiki), both beside the main checkout - also from a worktree.
#
# Takes the build lock through run-windows.sh - never wrap it in flock. Close a running
# rcommander.exe first: it locks the file the build replaces.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENES="$ROOT/scripts/screenshots/scenes.json"
MANIFEST="screenshots.json"

usage() {
    echo "usage: scripts/screenshots.sh [--release] [--deliver] [--timeout <seconds>]" >&2
    exit 2
}

# Sets RELEASE, DELIVER and TIMEOUT from the arguments.
parse_args() {
    RELEASE=0
    DELIVER=0
    TIMEOUT=600
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --release) RELEASE=1; shift ;;
            --deliver) DELIVER=1; shift ;;
            --timeout)
                [[ $# -ge 2 && "$2" =~ ^[1-9][0-9]{0,4}$ ]] || usage
                TIMEOUT="$2"
                shift 2
                ;;
            *) usage ;;
        esac
    done
}

# The main checkout, also from a feature worktree: the website and the wiki lie beside it.
main_checkout() {
    local common
    if common="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2> /dev/null)"; then
        dirname "$common"
    else
        echo "$ROOT"
    fi
}

# The folders the pictures go to: the artifacts of <version>, the website's and the wiki's.
artifacts_dir() { echo "$ROOT/artifacts/screenshots/$1"; }
website_dir() { echo "${RC_WEBSITE_DIR:-$(main_checkout)/../rcommander-website}/public/screenshots"; }
wiki_dir() { echo "${RC_WIKI_DIR:-$(main_checkout)/../rcommander.wiki}/images/screenshots"; }

# The PNGs a manifest lists, one name a line, after checking it: a run that failed, a name that
# is no plain <scene>.png or a picture missing in <out> fails.
listed_shots() {
    local manifest="$1" out="$2"
    python3 -B -I - "$manifest" "$out" << 'EOF'
import json, os, re, sys
manifest, out = sys.argv[1], sys.argv[2]
with open(manifest, encoding="utf-8") as handle:
    data = json.load(handle)
if data.get("ok") is not True:
    sys.exit(f"the run failed: {data.get('error', 'no reason given')}")
shots = data.get("shots")
if not isinstance(shots, list) or not shots:
    sys.exit("the manifest lists no pictures")
for shot in shots:
    name = shot.get("file") if isinstance(shot, dict) else None
    if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}\.png", name):
        sys.exit(f"the manifest names {name!r}, no plain <scene>.png")
    if not os.path.isfile(os.path.join(out, name)):
        sys.exit(f"{name} is listed but missing")
    print(name)
EOF
}

# Copies the pictures of <manifest> from <out> into <dest>, replacing the PNGs there, and the
# manifest beside them.
collect() {
    local manifest="$1" out="$2" dest="$3" names name
    names="$(listed_shots "$manifest" "$out")" || return 1
    mkdir -p "$dest"
    find "$dest" -maxdepth 1 -type f -name '*.png' -delete
    while IFS= read -r name; do
        cp "$out/$name" "$dest/$name"
    done <<< "$names"
    cp "$manifest" "$dest/$MANIFEST"
}

# Copies the PNGs of <from> into the website's and the wiki's folders; a checkout that is not
# there is refused, nothing is copied then.
deliver() {
    local from="$1" target repo
    for target in "$(website_dir)" "$(wiki_dir)"; do
        repo="$(dirname "$(dirname "$target")")"
        if [[ ! -d "$repo" ]]; then
            echo "!! $repo is not there: nothing delivered" >&2
            return 1
        fi
    done
    for target in "$(website_dir)" "$(wiki_dir)"; do
        mkdir -p "$target"
        cp "$from"/*.png "$target/"
        echo "==> delivered to $target"
    done
}

main() {
    parse_args "$@"
    # shellcheck source=lib/workspace-version.sh
    source "$ROOT/scripts/lib/workspace-version.sh"
    local version cmd localappdata work windows_work demo_root start waited
    version="$(rc_workspace_version < "$ROOT/Cargo.toml")"
    [[ -n "$version" ]] || { echo "!! no workspace version in Cargo.toml" >&2; exit 1; }
    cmd="$(command -v cmd.exe || echo /mnt/c/Windows/System32/cmd.exe)"
    if ! command -v wslpath > /dev/null 2>&1 || [[ ! -e "$cmd" ]]; then
        echo "!! wslpath or cmd.exe not found: this needs WSL" >&2
        exit 1
    fi
    # cmd.exe started from a WSL directory warns about UNC paths; from /mnt/c it does not.
    localappdata="$(cd /mnt/c && "$cmd" /d /c 'echo %LOCALAPPDATA%' 2> /dev/null | tr -d '\r')"
    if [[ -z "$localappdata" || "$localappdata" == "%LOCALAPPDATA%" ]]; then
        echo "!! could not read %LOCALAPPDATA% through cmd.exe" >&2
        exit 1
    fi
    work="$(wslpath -u "$localappdata")/rcommander-dev/screenshots"
    rm -rf "$work/out"
    mkdir -p "$work/out"
    cp "$SCENES" "$work/scenes.json"
    windows_work="$(wslpath -w "$work")"
    demo_root="${RC_SHOTS_DEMO_ROOT:-C:\\rCommander-Demo}"

    local build=()
    [[ "$RELEASE" -eq 1 ]] && build=(--release)
    "$ROOT/scripts/run-windows.sh" "${build[@]}" -- --screenshots "$windows_work\\scenes.json" \
        --out "$windows_work\\out" --demo-root "$demo_root"

    echo "==> waiting up to ${TIMEOUT}s for $windows_work\\out\\$MANIFEST"
    start="$SECONDS"
    while [[ ! -f "$work/out/$MANIFEST" ]]; do
        waited=$((SECONDS - start))
        if [[ "$waited" -ge "$TIMEOUT" ]]; then
            echo "!! no $MANIFEST after ${TIMEOUT}s; the window may still be open" >&2
            exit 1
        fi
        sleep 2
    done
    # The window writes the manifest last, after it closed; a moment for the file to settle.
    sleep 1

    local dest
    dest="$(artifacts_dir "$version")"
    collect "$work/out/$MANIFEST" "$work/out" "$dest"
    echo "==> $(find "$dest" -maxdepth 1 -name '*.png' | wc -l | tr -d ' ') pictures in $dest"
    if [[ "$DELIVER" -eq 1 ]]; then
        deliver "$dest"
    fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
