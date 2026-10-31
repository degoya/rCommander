#!/usr/bin/env bash
#
# The steps of release.yml's channel jobs that follow a published release out of the repository
# (RC-100-52, RC-100-53): the Homebrew tap, the Scoop bucket and winget. The apt and dnf
# repository is scripts/release-repositories.sh's (RC-100-54). CI only, from the checkout root of
# the tag; every channel moves for the newest plain vX.Y.Z tag only, so a re-run of an old tag or
# a pre-release cannot move one back. Adapted from rDownloader's scripts/release-channels.sh
# (RD-180-06, RD-180-07, RD-1101-07) at commit 8c30caac8.
#
#   scripts/release-channels.sh newest          # the newest plain vX.Y.Z tag of origin
#   scripts/release-channels.sh gate <channel>  # tap | bucket | winget: output move=true|false
#   scripts/release-channels.sh render          # rendered/ from the release's SHA256SUMS
#   scripts/release-channels.sh push            # CHECKOUT and RENDERED: one commit, pushed
#   scripts/release-channels.sh winget          # komac's pull request to microsoft/winget-pkgs
#   scripts/release-channels.sh install-nfpm    # nfpm, checksum-pinned, on GITHUB_PATH
#
# Every command reads REF_NAME (the tag) from the step's environment; `gate` the channel's
# secret (HOMEBREW_TAP_DEPLOY_KEY, SCOOP_BUCKET_DEPLOY_KEY, WINGET_TOKEN), `render` GH_TOKEN,
# `winget` KOMAC_VERSION, KOMAC_SHA256, GITHUB_TOKEN and GH_TOKEN (the WINGET_TOKEN) - never from
# the command line, and nothing here prints one. A channel whose secret is missing (a fork, or
# before the owner made it) is skipped with a notice; the release is out by then, so the jobs
# continue on error and never fail it.
set -euo pipefail

# Only plain vX.Y.Z tags count, and sort -V orders v1.10.0 after v1.9.0.
newest() {
    git ls-remote --tags --refs origin 'refs/tags/v*' \
        | sed 's#.*refs/tags/##' | { grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' || true; } \
        | sort -V | tail -n 1
}

# A secret cannot be tested in `if:`, so this says whether the channel has its key.
gate() {
    local channel="${1:-}" secret newest
    case "$channel" in
        tap) secret=HOMEBREW_TAP_DEPLOY_KEY ;;
        bucket) secret=SCOOP_BUCKET_DEPLOY_KEY ;;
        winget) secret=WINGET_TOKEN ;;
        *) echo "usage: scripts/release-channels.sh gate tap|bucket|winget" >&2; exit 2 ;;
    esac
    newest="$(newest)"
    if [[ "${REF_NAME}" != "${newest}" ]]; then
        echo "::notice::${REF_NAME} is not the newest release tag (${newest:-none}); the ${channel} stays where it is"
        echo "move=false" >> "${GITHUB_OUTPUT}"
    elif [[ -z "${!secret:-}" ]]; then
        echo "::notice::${secret} is not set; the ${channel} stays on its last release"
        echo "move=false" >> "${GITHUB_OUTPUT}"
    else
        echo "move=true" >> "${GITHUB_OUTPUT}"
    fi
}

render() {
    gh release download "${REF_NAME}" --repo "${GITHUB_REPOSITORY}" --pattern SHA256SUMS --dir release
    scripts/package-managers.sh "${REF_NAME}" release/SHA256SUMS rendered --repository "${GITHUB_REPOSITORY}"
}

# The key stays where actions/checkout put it, in that checkout's SSH configuration; nothing here
# reads or prints it. RENDERED is the folder laid out as the repository holds it.
push() {
    cp -R "${RENDERED}/." "${CHECKOUT}/"
    git -C "${CHECKOUT}" config user.name "github-actions[bot]"
    git -C "${CHECKOUT}" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
    git -C "${CHECKOUT}" add --all
    if git -C "${CHECKOUT}" diff --cached --quiet; then
        echo "${CHECKOUT}: already at ${REF_NAME}"
        exit 0
    fi
    git -C "${CHECKOUT}" commit --quiet --message "rcommander ${REF_NAME#v}"
    git -C "${CHECKOUT}" push --quiet origin HEAD
    echo "${CHECKOUT}: rcommander ${REF_NAME#v}"
}

# komac opens the pull request from the fork <account>/winget-pkgs of the token's account, which
# must exist; the token reaches komac as GITHUB_TOKEN and gh as GH_TOKEN, never on a command line.
# The fork is synced with microsoft/winget-pkgs first, and a refused branch - GitHub sometimes
# refuses komac's ref in a fork far behind (rDownloader's RD-190-10) - is tried again after a
# pause; any other failure is not, so no pull request doubles.
winget() {
    local archive fork attempt
    archive="komac-${KOMAC_VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    curl --fail --silent --show-error --location --retry 3 --output "${RUNNER_TEMP}/${archive}" \
        "https://github.com/russellbanks/Komac/releases/download/v${KOMAC_VERSION}/${archive}"
    echo "${KOMAC_SHA256}  ${RUNNER_TEMP}/${archive}" | sha256sum --check --quiet
    tar --extract --gzip --file "${RUNNER_TEMP}/${archive}" --directory "${RUNNER_TEMP}" komac
    fork="$(gh api user --jq .login)/winget-pkgs"
    for attempt in 1 2 3; do
        if ! gh api --method POST "repos/${fork}/merge-upstream" -f branch=master > /dev/null; then
            echo "::warning::${fork} could not be synced with microsoft/winget-pkgs"
        fi
        if "${RUNNER_TEMP}/komac" submit rendered/winget --yes 2>&1 | tee "${RUNNER_TEMP}/komac.log"; then
            exit 0
        fi
        if [[ "${attempt}" -eq 3 ]] || ! grep -q 'failed to create branch' "${RUNNER_TEMP}/komac.log"; then
            exit 1
        fi
        echo "::warning::GitHub refused komac's branch in ${fork} (attempt ${attempt}); trying again in 60 s"
        sleep 60
    done
}

# nfpm writes both formats for both architectures and runs nothing it packs. Pinned with its
# published checksum (rDownloader's pin).
install_nfpm() {
    curl --fail --silent --show-error --location --output "${RUNNER_TEMP}/nfpm.tar.gz" \
        https://github.com/goreleaser/nfpm/releases/download/v2.47.0/nfpm_2.47.0_Linux_x86_64.tar.gz
    echo "0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783  ${RUNNER_TEMP}/nfpm.tar.gz" | sha256sum --check
    tar --extract --gzip --file "${RUNNER_TEMP}/nfpm.tar.gz" --directory "${RUNNER_TEMP}" nfpm
    echo "${RUNNER_TEMP}" >> "${GITHUB_PATH}"
}

case "${1:-}" in
    newest) newest ;;
    gate) gate "${2:-}" ;;
    render) render ;;
    push) push ;;
    winget) winget ;;
    install-nfpm) install_nfpm ;;
    *) echo "usage: scripts/release-channels.sh newest|gate <channel>|render|push|winget|install-nfpm" >&2; exit 2 ;;
esac
