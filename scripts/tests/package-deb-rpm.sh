#!/usr/bin/env bash
#
# scripts/package-deb-rpm.sh --render-only against a fake release tarball (RC-100-54): the nfpm
# configs of both formats name the tarball's version and nfpm's architecture, put the program,
# its plugins, VERSION.txt and the install-kind marker into /usr/lib/rcommander with the
# /usr/bin link, ship the menu entry and the hicolor icons from 48 to 512 px and the SVG, and keep
# no placeholder; the marker names its format; a tarball named for another version, a test name
# or a missing program is refused. nfpm itself runs in release.yml and packages-repo.yml.
#
#   scripts/tests/package-deb-rpm.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

if ! python3 -I -c 'import yaml' 2> /dev/null; then
    echo "skip package-deb-rpm.sh: no PyYAML on this machine"
    exit 0
fi

# A tarball laid out as scripts/package-linux.sh writes it: $1 the version, $2 the arch.
tarball() {
    local stage="$SCRATCH/build/rcommander-$1-linux-$2"
    mkdir -p "$stage/plugins"
    echo program > "$stage/rcommander"
    echo plugin > "$stage/plugins/viewer-1.0.0.rcplug"
    printf '%s\n' "rCommander ${3:-$1}" "commit 1a2b3c4d" "built 2026-10-31T00:00:00Z" \
        "target x86_64-unknown-linux-gnu, profile release" > "$stage/VERSION.txt"
    touch "$stage/README.md" "$stage/LICENSE" "$stage/NOTICE"
    tar -C "$SCRATCH/build" -czf "$SCRATCH/rcommander-$1-linux-$2.tar.gz" "rcommander-$1-linux-$2"
    rm -rf "$SCRATCH/build"
    echo "$SCRATCH/rcommander-$1-linux-$2.tar.gz"
}
# One value of a rendered config, by a Python expression over `c`.
config() { python3 -I -c "import sys, yaml; c = yaml.safe_load(open(sys.argv[1])); print($2)" "$1"; }
render() { "$ROOT/scripts/package-deb-rpm.sh" --render-only "$@"; }

run_status render "$(tarball 1.6.0 x64)" "$SCRATCH/out"
expect_status "renders the configs of an x64 tarball" 0
deb="$SCRATCH/out/nfpm-deb-x64.yaml"
rpm="$SCRATCH/out/nfpm-rpm-x64.yaml"
expect_true "one config per format" '[[ -f "$deb" && -f "$rpm" ]]'
expect "the version of VERSION.txt" "1.6.0" "$(config "$deb" 'c["version"]')"
expect "nfpm's architecture" "amd64" "$(config "$deb" 'c["arch"]')"
expect "no placeholder is left" "0" "$(cat "$deb" "$rpm" | grep -c '@[A-Z0-9_]*@' || true)"
expect "the program, its plugins, VERSION.txt, the marker and the link" \
    "/usr/bin/rcommander /usr/lib/rcommander /usr/lib/rcommander/VERSION.txt /usr/lib/rcommander/install-kind /usr/lib/rcommander/plugins /usr/lib/rcommander/rcommander" \
    "$(config "$deb" '" ".join(sorted(e["dst"] for e in c["contents"] if "/usr/lib/" in e["dst"] or e["dst"].startswith("/usr/bin")))')"
expect "the link points at the program" "/usr/lib/rcommander/rcommander" \
    "$(config "$deb" '[e["src"] for e in c["contents"] if e["dst"] == "/usr/bin/rcommander"][0]')"
expect "the menu entry" "1" "$(config "$deb" 'sum(e["dst"] == "/usr/share/applications/rcommander.desktop" for e in c["contents"])')"
expect "the hicolor icons" \
    "128x128 256x256 48x48 512x512 64x64 scalable" \
    "$(config "$deb" '" ".join(sorted(e["dst"].split("/")[5] for e in c["contents"] if "/hicolor/" in e["dst"]))')"
expect "the deb's glibc floor" "libc6 (>= 2.39)" "$(config "$deb" 'c["depends"][0]')"
expect "the rpm's glibc floor" "glibc >= 2.39" "$(config "$rpm" 'c["overrides"]["rpm"]["depends"][0]')"
expect_true "the menu entry exists" '[[ -f "$ROOT/packaging/linux/rcommander.desktop" ]]'
expect "the menu entry starts the command with the icon" "Exec=rcommander Icon=rcommander" \
    "$(grep -E '^(Exec|Icon)=' "$ROOT/packaging/linux/rcommander.desktop" | tr '\n' ' ' | sed 's/ $//')"

run_status render "$(tarball 1.6.0 arm64)" "$SCRATCH/arm" --rpm
expect_status "an arm64 tarball, the rpm only" 0
expect "arm64 is arm64 for nfpm" "arm64" "$(config "$SCRATCH/arm/nfpm-rpm-arm64.yaml" 'c["arch"]')"
expect_true "and no deb config" '[[ ! -e "$SCRATCH/arm/nfpm-deb-arm64.yaml" ]]'

run_status render "$(tarball 1.6.0 x64 1.5.0)" "$SCRATCH/mismatch"
expect_status "a tarball named for another version than its VERSION.txt is refused" 1
touch "$SCRATCH/rcommander-linux.tar.gz"
run_status render "$SCRATCH/rcommander-linux.tar.gz" "$SCRATCH/bad-name"
expect_status "a file that is no release tarball is refused" 2
run_status render "$SCRATCH/rcommander-9.9.9-linux-x64.tar.gz" "$SCRATCH/missing"
expect_status "a missing tarball is refused" 1
run_status render "$(tarball 1.6.0 x64)"
expect_status "too few arguments is a usage error" 2
run_status render "$(tarball 1.6.0 x64)" "$SCRATCH/x" --zip
expect_status "an unknown format is a usage error" 2

finish_tests "package-deb-rpm.sh"
