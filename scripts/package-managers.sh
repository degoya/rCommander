#!/usr/bin/env bash
#
# The package managers' files of one release (RC-100-52, RC-100-53), rendered from packaging/
# with the version and the archives' SHA-256 from the release's SHA256SUMS: the Homebrew tap's
# cask (macOS) and formula (Linux), the Scoop bucket's manifest, each with its repository's
# README, and the three winget manifests. Adapted from rDownloader's scripts/package-managers.sh
# (RD-180-06, RD-180-07) at commit 8c30caac8.
#
# Usage:
#   scripts/package-managers.sh <version> <SHA256SUMS> <outdir> [--repository OWNER/NAME]
#                               [--base-url URL]
#   scripts/package-managers.sh --check [--sums <SHA256SUMS>] [--repository OWNER/NAME]
#
# Writes the files as each repository holds them:
#   <outdir>/homebrew/Casks/rcommander.rb, Formula/rcommander.rb, README.md
#   <outdir>/scoop/bucket/rcommander.json, README.md
#   <outdir>/winget/degoya.rCommander.yaml, .installer.yaml, .locale.en-US.yaml
# The release workflow pushes the first two after the release is published and submits the third
# as a pull request to microsoft/winget-pkgs; package-channels.yml installs through all of them.
#
#   --repository   the GitHub repository whose releases they install, default degoya/rCommander;
#                  the tap and the bucket are <owner>/homebrew-rcommander and
#                  <owner>/scoop-rcommander
#   --base-url     where the archives are downloaded from, default
#                  https://github.com/<repository>/releases/download/v<version>; CI points it at
#                  a local fixture
#
# --check holds the templates against the last release: it renders them with the version and
# the SHA256SUMS of the repository's latest release (downloaded, or the file --sums names, its
# version read from the archive names) into a temporary folder, so a template that names an
# archive the release does not have fails here, before a release would publish it.
#
# Every archive the files name must be in SHA256SUMS (`<sha256>  ./<name>`, as
# scripts/release-assets.sh writes it, or `<sha256>  <name>`), and nothing of a template may stay
# unreplaced; a failure writes nothing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/render-template.sh
source "$ROOT/scripts/lib/render-template.sh"
# Bash 5.2 would put the matched text in place of a `&` in a substitution's replacement.
shopt -u patsub_replacement 2> /dev/null || true

usage() {
    echo "usage: $0 <version> <SHA256SUMS> <outdir> [--repository OWNER/NAME] [--base-url URL]" >&2
    echo "       $0 --check [--sums <SHA256SUMS>] [--repository OWNER/NAME]" >&2
    exit 2
}

valid_repository() {
    if [[ ! "$1" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
        echo "error: '$1' is not OWNER/NAME" >&2
        exit 1
    fi
}

if [[ "${1:-}" == --check ]]; then
    shift
    sums=""
    repository="degoya/rCommander"
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --sums) [[ $# -ge 2 ]] || usage; sums="$2"; shift 2 ;;
            --repository) [[ $# -ge 2 ]] || usage; repository="$2"; shift 2 ;;
            *) usage ;;
        esac
    done
    valid_repository "$repository"
    scratch="$(mktemp -d)"
    trap 'rm -rf "$scratch"' EXIT
    if [[ -z "$sums" ]]; then
        sums="$scratch/SHA256SUMS"
        url="https://github.com/$repository/releases/latest/download/SHA256SUMS"
        if ! curl --fail --silent --show-error --location --retry 3 --output "$sums" "$url"; then
            echo "error: the last release's SHA256SUMS could not be downloaded ($url)" >&2
            exit 1
        fi
    fi
    [[ -f "$sums" ]] || { echo "error: $sums does not exist" >&2; exit 1; }
    version="$(sed -n 's#^[0-9a-fA-F]\{64\}  *[*]\{0,1\}\(\./\)\{0,1\}rcommander-\(.*\)-windows-x64\.zip$#\2#p' "$sums" | head -n 1)"
    if [[ -z "$version" ]]; then
        echo "error: $sums names no rcommander-<version>-windows-x64.zip; its version is unknown" >&2
        exit 1
    fi
    if "$0" "$version" "$sums" "$scratch/out" --repository "$repository" > "$scratch/log" 2>&1; then
        echo "the templates render against the last release, $version: $(grep -c '^wrote ' "$scratch/log") files"
        exit 0
    fi
    cat "$scratch/log" >&2
    echo "error: the templates do not render against the last release, $version" >&2
    exit 1
fi

[[ $# -ge 3 ]] || usage
version="${1#v}"
sums="$2"
outdir="$3"
shift 3
repository="degoya/rCommander"
base_url=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --repository) [[ $# -ge 2 ]] || usage; repository="$2"; shift 2 ;;
        --base-url) [[ $# -ge 2 ]] || usage; base_url="$2"; shift 2 ;;
        *) usage ;;
    esac
done

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "error: '$version' is not a release version (X.Y.Z or X.Y.Z-suffix)" >&2
    exit 1
fi
valid_repository "$repository"
[[ -f "$sums" ]] || { echo "error: $sums does not exist" >&2; exit 1; }
owner="${repository%%/*}"
base_url="${base_url:-https://github.com/${repository}/releases/download/v${version}}"
base_url="${base_url%/}"

# The hash of archive $1 in SHA256SUMS, lower case; nothing when it is not there. macOS's
# /bin/bash is 3.2: no associative arrays, so a lookup by awk.
hash_of() {
    awk -v wanted="$1" '{
        name = $2
        sub(/^\*/, "", name)
        sub(/^\.\//, "", name)
        if (name == wanted) { print tolower($1); exit }
    }' "$sums"
}

values=(
    "VERSION=$version"
    "REPOSITORY=$repository"
    "OWNER=$owner"
    "BASE_URL=$base_url"
    "TAP=${owner}/homebrew-rcommander"
    "BUCKET=${owner}/scoop-rcommander"
)
# One placeholder per archive the files install.
missing=0
for pair in \
    SHA256_MACOS_AARCH64="rcommander-$version-macos-aarch64.zip" \
    SHA256_MACOS_X86_64="rcommander-$version-macos-x86_64.zip" \
    SHA256_LINUX_ARM64="rcommander-$version-linux-arm64.tar.gz" \
    SHA256_LINUX_X64="rcommander-$version-linux-x64.tar.gz" \
    SHA256_WINDOWS_X64="rcommander-$version-windows-x64.zip"; do
    archive="${pair#*=}"
    hash="$(hash_of "$archive")"
    if [[ ! "$hash" =~ ^[0-9a-f]{64}$ ]]; then
        echo "error: $sums has no SHA-256 for $archive" >&2
        missing=1
        continue
    fi
    values+=("${pair%%=*}=$hash")
    # winget-pkgs writes InstallerSha256 in upper case.
    if [[ "${pair%%=*}" == SHA256_WINDOWS_X64 ]]; then
        values+=("SHA256_WINDOWS_X64_UPPER=$(printf '%s' "$hash" | tr 'a-f' 'A-F')")
    fi
done
[[ "$missing" -eq 0 ]] || exit 1

render() {
    mkdir -p "$(dirname "$2")"
    rc_render_template "$1" "$2" "${values[@]}" || exit 1
    echo "wrote $2"
}

packaging="$ROOT/packaging"
render "$packaging/homebrew/rcommander-cask.rb.in" "$outdir/homebrew/Casks/rcommander.rb"
render "$packaging/homebrew/rcommander.rb.in" "$outdir/homebrew/Formula/rcommander.rb"
render "$packaging/homebrew/README.md.in" "$outdir/homebrew/README.md"
render "$packaging/scoop/rcommander.json.in" "$outdir/scoop/bucket/rcommander.json"
render "$packaging/scoop/README.md.in" "$outdir/scoop/README.md"
for manifest in degoya.rCommander.yaml degoya.rCommander.installer.yaml \
    degoya.rCommander.locale.en-US.yaml; do
    render "$packaging/winget/$manifest.in" "$outdir/winget/$manifest"
done
