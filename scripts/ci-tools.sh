#!/usr/bin/env bash
#
# The pinned downloads of ci.yml's `checks` job (RC-100-31, adapted from rDownloader's script of
# the same name): shellcheck and gitleaks into $RUNNER_TEMP/ci-bin, put on PATH, each checked
# against its SHA-256 from scripts/lib/tool-versions.sh before it is unpacked - so
# `scripts/check.sh --preflight` runs both stages instead of skipping them. CI only, Linux x86_64.
#
#   scripts/ci-tools.sh linters
set -euo pipefail

# shellcheck source=lib/tool-versions.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/tool-versions.sh"

linters() {
    local bin="${RUNNER_TEMP:?set by the runner}/ci-bin" archive
    mkdir -p "$bin"
    archive="$RUNNER_TEMP/shellcheck.tar.xz"
    curl -fsSL --retry 3 -o "$archive" \
        "https://github.com/koalaman/shellcheck/releases/download/v${RC_SHELLCHECK_VERSION}/shellcheck-v${RC_SHELLCHECK_VERSION}.linux.x86_64.tar.xz"
    echo "$RC_SHELLCHECK_SHA256  $archive" | sha256sum --check --strict
    tar -xJf "$archive" -C "$bin" --strip-components=1 "shellcheck-v${RC_SHELLCHECK_VERSION}/shellcheck"
    archive="$RUNNER_TEMP/gitleaks.tar.gz"
    curl -fsSL --retry 3 -o "$archive" \
        "https://github.com/gitleaks/gitleaks/releases/download/v${RC_GITLEAKS_VERSION}/gitleaks_${RC_GITLEAKS_VERSION}_linux_x64.tar.gz"
    echo "$RC_GITLEAKS_SHA256  $archive" | sha256sum --check --strict
    tar -xzf "$archive" -C "$bin" gitleaks
    echo "$bin" >> "${GITHUB_PATH:?set by the runner}"
}

case "${1:-}" in
    linters) linters ;;
    *) echo "usage: scripts/ci-tools.sh linters" >&2; exit 2 ;;
esac
