#!/usr/bin/env bash
#
# The checksums of a release's files (RC-100-32), as the public repository's release.yml calls it
# once every asset is in one directory:
#
#   scripts/release-assets.sh sums <directory>
#
# Writes `<directory>/SHA256SUMS` over every regular file in it but `SHA256SUMS*`, sorted by name,
# so `sha256sum --check SHA256SUMS` verifies a full download. Run before
# `scripts/release-publish.sh update-manifest`: the update manifest is signed itself and stays out
# of the sums. Exit 1 when the directory holds no file to sum, 2 on a wrong call.
set -euo pipefail

usage() {
    echo "usage: scripts/release-assets.sh sums <directory>" >&2
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

case "${1:-}" in
    sums)
        [[ $# -eq 2 ]] || usage
        sums "$2"
        ;;
    *) usage ;;
esac
