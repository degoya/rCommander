#!/usr/bin/env bash
#
# The manual variant of the release chain, in the order it has to happen: version, checks,
# packages - no commit, no tag, no push (RC-100-32, as rDownloader). A release is cut with
# scripts/release-pipeline.sh; this one stays for building a package set by hand.
#
# Usage:
#   scripts/release.sh            # check and package the current version
#   scripts/release.sh 0.9.0      # bump to 0.9.0 first (scripts/set-version.sh)
#
# Takes the build lock (scripts/lib/lock.sh) for the whole run; never wrap it in flock.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Sourced before the `cd`, because the lock library resolves this script's own path from $0.
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"
cd "$ROOT"

[[ $# -le 1 ]] || { echo "usage: scripts/release.sh [X.Y.Z]" >&2; exit 2; }
[[ $# -eq 0 ]] || scripts/set-version.sh "$1"
current="$(scripts/set-version.sh)"
echo "==> packaging $current"

scripts/check.sh --full
scripts/package-windows.sh --profile release
scripts/package-linux.sh --profile release --arch x64
scripts/package-linux.sh --profile release --arch arm64

cat << SUMMARY

==> $current is packaged in artifacts/windows/ and artifacts/linux/

    Still by hand, because they are judgement rather than mechanics:
      - CHANGELOG.md, RELEASE-NOTES.md and the job files
      - committing the result
    The tag only comes from scripts/release-pipeline.sh, which keeps the evidence it needs.
SUMMARY
