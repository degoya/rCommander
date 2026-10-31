#!/usr/bin/env bash
#
# The steps of release.yml's `apt-dnf` job (RC-100-54): the release's .deb and .rpm packages into
# the repository <owner>/rcommander-packages, the tree of the signed apt and dnf repository. The
# filing, pruning and signing is scripts/package-repo.sh; this is the gate, the clone and the push
# around it. CI only, from the checkout root of the tag; the newest plain vX.Y.Z tag alone moves
# the repository. Adapted from rDownloader's scripts/release-repositories.sh (RD-180-10) at commit
# 8c30caac8.
#
#   scripts/release-repositories.sh gate       # output publish: true|false
#   scripts/release-repositories.sh download   # incoming/*.deb, *.rpm; output found: true|false
#   scripts/release-repositories.sh clone      # site/, and GIT_SSH_COMMAND for the later steps
#   scripts/release-repositories.sh sign       # file, prune and sign into site/
#   scripts/release-repositories.sh push       # site/ as one commit on `main`
#
# The repository is served by GitHub Pages from its `main`, https://<owner>.github.io/rcommander-packages
# (owner 2026-10-31); the repository variable PACKAGES_BASE_URL may name another address that
# serves the same tree (rcommander.net/packages), and the .sources, the .repo, the README and the
# page then name that one.
#
# The signing key is the one whose public half is packaging/repository/rcommander-packages.asc:
# its fingerprint is read from that file, and a secret of any other key signs nothing.
#
# Reads REF_NAME, OWNER, PACKAGES_DEPLOY_KEY (the write deploy key of rcommander-packages),
# PACKAGES_GPG_KEY (the ASCII-armoured secret key rcommander-packages, without a passphrase, as
# rDownloader's), PACKAGES_GPG_PASSPHRASE (optional, unset: only for a key that has one) and
# PACKAGES_BASE_URL (may be empty) from the step's environment; nothing prints a key or a
# passphrase.
set -euo pipefail

gate() {
    local newest secret
    newest="$(scripts/release-channels.sh newest)"
    if [[ "${REF_NAME}" != "${newest}" ]]; then
        echo "::notice::${REF_NAME} is not the newest release tag (${newest:-none}); the apt and dnf repository stays where it is"
        echo "publish=false" >> "${GITHUB_OUTPUT}"
        exit 0
    fi
    for secret in PACKAGES_DEPLOY_KEY PACKAGES_GPG_KEY; do
        if [[ -z "${!secret:-}" ]]; then
            echo "::notice::${secret} is not set; the apt and dnf repository stays on its last release"
            echo "publish=false" >> "${GITHUB_OUTPUT}"
            exit 0
        fi
    done
    echo "publish=true" >> "${GITHUB_OUTPUT}"
}

download() {
    gh release download "${REF_NAME}" --repo "${GITHUB_REPOSITORY}" \
        --pattern '*.deb' --pattern '*.rpm' --dir incoming || true
    if ! compgen -G 'incoming/*.deb' > /dev/null && ! compgen -G 'incoming/*.rpm' > /dev/null; then
        echo "::warning::${REF_NAME} carries no .deb or .rpm; the apt and dnf repository stays on its last release"
        echo "found=false" >> "${GITHUB_OUTPUT}"
        exit 0
    fi
    ls -l incoming
    echo "found=true" >> "${GITHUB_OUTPUT}"
}

# GitHub's published SSH host key, pinned as actions/checkout pins it. Cloning an empty repository
# succeeds with a warning, and the first push creates `main`; hence by hand, not actions/checkout.
clone() {
    local ssh_command
    install -m 0600 /dev/null "${RUNNER_TEMP}/packages-deploy-key"
    printf '%s\n' "${PACKAGES_DEPLOY_KEY}" > "${RUNNER_TEMP}/packages-deploy-key"
    echo 'github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl' \
        > "${RUNNER_TEMP}/github-known-hosts"
    ssh_command="ssh -i ${RUNNER_TEMP}/packages-deploy-key -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=${RUNNER_TEMP}/github-known-hosts"
    echo "GIT_SSH_COMMAND=${ssh_command}" >> "${GITHUB_ENV}"
    GIT_SSH_COMMAND="${ssh_command}" git clone --quiet --depth 1 \
        "git@github.com:${OWNER}/rcommander-packages.git" site
}

# The key goes from the secret into a keyring of this run only, an optional passphrase into a file
# only the run's user reads; both are gone with the runner.
sign() {
    local fingerprint options=()
    fingerprint="$(gpg --batch --with-colons --show-keys packaging/repository/rcommander-packages.asc \
        | awk -F: '$1 == "fpr" { print $10; exit }')"
    [[ "${fingerprint}" =~ ^[0-9A-F]{40}$ ]] || {
        echo "::error::packaging/repository/rcommander-packages.asc names no key"
        exit 1
    }
    export GNUPGHOME="${RUNNER_TEMP}/gnupg"
    install -d -m 0700 "${GNUPGHOME}"
    printf '%s\n' "${PACKAGES_GPG_KEY}" | gpg --batch --quiet --import
    if [[ -n "${PACKAGES_GPG_PASSPHRASE:-}" ]]; then
        install -m 0600 /dev/null "${RUNNER_TEMP}/packages-passphrase"
        printf '%s\n' "${PACKAGES_GPG_PASSPHRASE}" > "${RUNNER_TEMP}/packages-passphrase"
        options+=(--passphrase-file "${RUNNER_TEMP}/packages-passphrase")
    fi
    scripts/package-repo.sh incoming site --key "${fingerprint}" "${options[@]}" \
        --base-url "${PACKAGES_BASE_URL:-https://${OWNER,,}.github.io/rcommander-packages}" \
        --repository "${GITHUB_REPOSITORY}"
    rm -f "${RUNNER_TEMP}/packages-passphrase"
    gpgconf --kill all
}

# `main` is replaced by one commit each time: the pool is the archive, not the history, and a
# history of every package ever published would outgrow the repository.
push() {
    local large size_mb
    # GitHub refuses files over 100 MB, and Pages publishes at most 1 GB.
    large="$(find site -path site/.git -prune -o -type f -size +99M -print)"
    if [[ -n "${large}" ]]; then
        echo "::error::over GitHub's 100 MB file limit: ${large//$'\n'/, }"
        exit 1
    fi
    size_mb="$(du -sm --exclude=.git site | cut -f1)"
    if [[ "${size_mb}" -gt 950 ]]; then
        echo "::error::the repository is ${size_mb} MB, GitHub Pages publishes at most 1 GB; lower --keep"
        exit 1
    fi
    cd site
    if [[ -z "$(git status --porcelain)" ]] && git rev-parse --verify --quiet HEAD > /dev/null; then
        echo "the apt and dnf repository is already at ${REF_NAME}"
        exit 0
    fi
    git checkout --quiet --orphan publish
    git add --all
    git config user.name "github-actions[bot]"
    git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
    git commit --quiet --message "rcommander ${REF_NAME#v}"
    git push --quiet --force origin HEAD:refs/heads/main
    echo "the apt and dnf repository: rcommander ${REF_NAME#v} (${size_mb} MB)"
}

case "${1:-}" in
    gate) gate ;;
    download) download ;;
    clone) clone ;;
    sign) sign ;;
    push) push ;;
    *) echo "usage: scripts/release-repositories.sh gate|download|clone|sign|push" >&2; exit 2 ;;
esac
