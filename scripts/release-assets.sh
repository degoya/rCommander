#!/usr/bin/env bash
#
# The checksums and the SBOMs of a release's files (RC-100-32, RC-110-73), as the public
# repository's release.yml calls them:
#
#   scripts/release-assets.sh sums <directory>
#   scripts/release-assets.sh sbom <directory> <version>
#
# `sums` writes `<directory>/SHA256SUMS` over every regular file in it but `SHA256SUMS*`, sorted by
# name, so `sha256sum --check SHA256SUMS` verifies a full download. Run before
# `scripts/release-publish.sh update-manifest`: the update manifest is signed itself and stays out
# of the sums. Exit 1 when the directory holds no file to sum, 2 on a wrong call.
#
# `sbom` writes the CycloneDX SBOMs (JSON, spec 1.5, the dependencies of every target) of the
# workspace at Cargo.lock: `<directory>/rcommander-<version>.cdx.json` for the application and
# `<directory>/rcommander-<version>-plugins.cdx.tar.gz` with one `<package>.cdx.json` per bundled
# plugin (every `plugins/*/` with a `manifest.toml`). cargo-cyclonedx at the pin of
# `lib/tool-versions.sh` (`RC_CARGO_CYCLONEDX_VERSION`) must be installed; it runs `cargo metadata`
# and compiles nothing. Its files beside the members' Cargo.toml are removed again. The archive is
# reproducible: names sorted, owner 0, the time SOURCE_DATE_EPOCH (else HEAD's commit time), which
# cargo-cyclonedx also writes as the SBOMs' timestamp. Run before `sums`, so SHA256SUMS lists them.
# Exit 1 when cargo-cyclonedx fails or leaves an SBOM missing, 2 on a wrong call.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    echo "usage: scripts/release-assets.sh sums <directory> | sbom <directory> <version>" >&2
    exit 2
}

sums() {
    local directory="$1" count
    [[ -d "$directory" ]] || { echo "release-assets: $directory is not a directory" >&2; exit 2; }
    (
        cd "$directory"
        find . -maxdepth 1 -type f ! -name 'SHA256SUMS*' -printf '%f\0' \
            | LC_ALL=C sort --zero-terminated \
            | xargs --null --no-run-if-empty sha256sum > SHA256SUMS
    )
    count="$(wc -l < "$directory/SHA256SUMS")"
    if [[ "$count" -eq 0 ]]; then
        rm -f "$directory/SHA256SUMS"
        echo "release-assets: $directory holds no file to sum" >&2
        exit 1
    fi
    echo "release-assets: $directory/SHA256SUMS lists $count file(s)"
}

# The SBOMs cargo-cyclonedx leaves beside the workspace members' Cargo.toml.
remove_member_sboms() {
    find "$ROOT/crates" "$ROOT/plugins" -mindepth 2 -maxdepth 2 -type f -name '*.cdx.json' -delete
}

# Fails unless file $1 is there and a CycloneDX document.
require_sbom() {
    [[ -f "$1" ]] || { echo "release-assets: cargo-cyclonedx wrote no $1" >&2; exit 1; }
    grep -q '"bomFormat"[[:space:]]*:[[:space:]]*"CycloneDX"' "$1" \
        || { echo "release-assets: $1 is no CycloneDX document" >&2; exit 1; }
}

sbom() {
    local directory="$1" version="$2" epoch staging manifest plugin found count=0
    [[ -d "$directory" ]] || { echo "release-assets: $directory is not a directory" >&2; exit 2; }
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] \
        || { echo "release-assets: '$version' is no version" >&2; exit 2; }
    directory="$(cd "$directory" && pwd)"
    epoch="${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct)}"
    staging="$(mktemp -d)"
    # shellcheck disable=SC2064  # the path is fixed now, on purpose
    trap "rm -rf '$staging'; remove_member_sboms" EXIT
    remove_member_sboms

    # The lock file as committed: cargo-cyclonedx's own `cargo metadata` passes no --locked.
    (cd "$ROOT" && cargo metadata --locked --format-version 1 > /dev/null)
    (cd "$ROOT" && SOURCE_DATE_EPOCH="$epoch" cargo cyclonedx --format json --target all --spec-version 1.5) \
        || { echo "release-assets: cargo cyclonedx failed" >&2; exit 1; }

    # Staged first: the directory gets both files or neither.
    require_sbom "$ROOT/crates/rcommander/rcommander.cdx.json"
    mkdir "$staging/plugins"
    for manifest in "$ROOT"/plugins/*/manifest.toml; do
        [[ -e "$manifest" ]] || continue
        plugin="$(dirname "$manifest")"
        found=("$plugin"/*.cdx.json)
        [[ ${#found[@]} -eq 1 ]] || { echo "release-assets: expected one SBOM in $plugin, found ${#found[@]}" >&2; exit 1; }
        require_sbom "${found[0]}"
        cp "${found[0]}" "$staging/plugins/"
        count=$((count + 1))
    done
    [[ "$count" -gt 0 ]] || { echo "release-assets: no bundled plugin under $ROOT/plugins" >&2; exit 1; }
    (
        cd "$staging/plugins"
        find . -maxdepth 1 -type f -name '*.cdx.json' -printf '%f\0' \
            | LC_ALL=C sort --zero-terminated \
            | tar --create --null --files-from=- --mtime="@$epoch" \
                --owner=0 --group=0 --numeric-owner --mode='u=rw,go=r' \
            | gzip -n -9 > "$staging/plugins.cdx.tar.gz"
    )
    cp "$ROOT/crates/rcommander/rcommander.cdx.json" "$directory/rcommander-$version.cdx.json"
    mv "$staging/plugins.cdx.tar.gz" "$directory/rcommander-$version-plugins.cdx.tar.gz"
    echo "release-assets: rcommander-$version.cdx.json and rcommander-$version-plugins.cdx.tar.gz ($count plugin(s)) in $directory"
}

case "${1:-}" in
    sums)
        [[ $# -eq 2 ]] || usage
        sums "$2"
        ;;
    sbom)
        [[ $# -eq 3 ]] || usage
        sbom "$2" "$3"
        ;;
    *) usage ;;
esac
