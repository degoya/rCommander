#!/usr/bin/env bash
#
# The halves of .github/workflows/packages-repo.yml (RC-100-54): the real deb and rpm packages of
# two releases, built from their published Linux tarballs with scripts/package-deb-rpm.sh, filed
# into two states of the repository with scripts/package-repo.sh and a throwaway key - states/1 holds the older release, states/2 adds the newer one - and then installed
# from as a user would, in each client's container. CI only; `install` and `refused` change the
# system they run on. Adapted from rDownloader's scripts/ci-package-repo.sh (RD-180-10) at commit
# 8c30caac8, with real packages in place of its fixtures.
#
#   scripts/ci-package-repo.sh build <old tag> <new tag>     # states/1, states/2 (needs gh, nfpm)
#   scripts/ci-package-repo.sh apt|dnf install <old> <new>   # install old, check, upgrade to new
#   scripts/ci-package-repo.sh apt|dnf refused <new>         # an altered signed index is refused
#
# `install` serves states/1 at file:///repo, adds the key and the .sources or .repo file exactly
# as the README says, installs, checks `rcommander --version`, the install-kind marker, the menu
# entry and its icons, upgrades to states/2 and checks the new version. `refused` shows that a
# repository whose signed index was altered fails the update with a non-zero exit and installs
# nothing - the signature is checked, nothing says trusted=yes, and dnf does not skip the
# repository with a warning - and that the same state unaltered installs again.
set -euxo pipefail

version_of() { rcommander --version | head -n 1; }

# The installation as the update check sees it: the marker, and `rcommander install-kind` where
# the release has it (it came with RC-100-52; an older release answers with clap's usage error).
check_installed() {
    local kind="$1" version="$2" said
    [[ "$(version_of)" == *"${version#v}"* ]]
    [[ "$(cat /usr/lib/rcommander/install-kind)" == "$kind" ]]
    [[ -L /usr/bin/rcommander ]]
    [[ -d /usr/lib/rcommander/plugins ]]
    [[ -f /usr/share/applications/rcommander.desktop ]]
    [[ -f /usr/share/icons/hicolor/scalable/apps/rcommander.svg ]]
    [[ -f /usr/share/icons/hicolor/512x512/apps/rcommander.png ]]
    if said="$(rcommander install-kind)"; then
        [[ "$(head -n 1 <<< "$said")" == "$kind" ]]
    else
        echo "::notice::rcommander ${version} has no install-kind subcommand yet; the marker alone is checked"
    fi
}

build() {
    local old="$1" new="$2" tag
    for tag in "$old" "$new"; do
        mkdir -p "tarballs/$tag" "incoming/$tag"
        gh release download "$tag" --repo "${GITHUB_REPOSITORY}" \
            --pattern 'rcommander-*-linux-x64.tar.gz' --pattern 'rcommander-*-linux-arm64.tar.gz' \
            --dir "tarballs/$tag"
        for tarball in "tarballs/$tag"/*.tar.gz; do
            scripts/package-deb-rpm.sh "$tarball" "incoming/$tag"
        done
    done
    export GNUPGHOME="$PWD/ci-gnupg"
    install -d -m 0700 "$GNUPGHOME"
    gpg --batch --quiet --passphrase '' --quick-gen-key 'rCommander CI <ci@example.invalid>' ed25519 sign 1d
    scripts/package-repo.sh "incoming/$old" site --base-url file:///repo
    mkdir -p states
    cp -a site states/1
    scripts/package-repo.sh "incoming/$new" site --base-url file:///repo
    cp -a site states/2
    gpgconf --kill all
    find states -maxdepth 4 -type d | sort
}

apt_install() {
    ln -sfn "$PWD/states/1" /repo
    install -d -m 0755 /etc/apt/keyrings
    cp /repo/rcommander-packages.asc /etc/apt/keyrings/rcommander-packages.asc
    cp /repo/rcommander.sources /etc/apt/sources.list.d/rcommander.sources
    # `!` would not stop the script under set -e, hence the if.
    if grep -i 'trusted=' /etc/apt/sources.list.d/rcommander.sources; then exit 1; fi
    apt-get update
    apt-get install -y rcommander
    check_installed deb "$1"
    ln -sfn "$PWD/states/2" /repo
    apt-get update
    apt-get upgrade -y
    check_installed deb "$2"
}

apt_refused() {
    cp -a states/2 tampered
    sed -i 's/^Label: rCommander$/Label: someone else/' \
        tampered/deb/dists/stable/InRelease tampered/deb/dists/stable/Release
    ln -sfn "$PWD/tampered" /repo
    apt-get purge -y rcommander
    rm -rf /var/lib/apt/lists/*
    if apt-get update; then
        echo "::error::apt-get update succeeds on a repository whose signed index was altered"
        exit 1
    fi
    if apt-get install -y rcommander; then
        echo "::error::apt installs rcommander from a repository whose signed index was altered"
        exit 1
    fi
    ln -sfn "$PWD/states/2" /repo
    apt-get update
    apt-get install -y rcommander
    check_installed deb "$1"
}

dnf_install() {
    ln -sfn "$PWD/states/1" /repo
    cp /repo/rcommander.repo /etc/yum.repos.d/rcommander.repo
    # gpgcheck=1 makes dnf refuse an unsigned rpm, so the install proves the package signature.
    grep -x 'gpgcheck=1' /etc/yum.repos.d/rcommander.repo
    grep -x 'repo_gpgcheck=1' /etc/yum.repos.d/rcommander.repo
    # Fedora's dnf5 skips an unavailable repository by default; the repository's own
    # skip_if_unavailable=0 turns a failed signature check into a hard error.
    grep -x 'skip_if_unavailable=0' /etc/yum.repos.d/rcommander.repo
    dnf install -y rcommander
    check_installed rpm "$1"
    ln -sfn "$PWD/states/2" /repo
    dnf upgrade -y --refresh rcommander
    check_installed rpm "$2"
}

dnf_refused() {
    cp -a states/2 tampered
    sed -i 's#<revision>#<revision>1#' tampered/rpm/repodata/repomd.xml
    ln -sfn "$PWD/tampered" /repo
    dnf remove -y rcommander
    dnf clean all
    if dnf makecache --refresh --repo rcommander; then
        echo "::error::dnf accepts a repomd.xml that no longer matches its signature"
        exit 1
    fi
    if dnf install -y --refresh rcommander; then
        echo "::error::dnf installs rcommander from a repository whose repomd.xml was altered"
        exit 1
    fi
    if rpm -q rcommander; then exit 1; fi
    ln -sfn "$PWD/states/2" /repo
    dnf install -y --refresh rcommander
    check_installed rpm "$1"
}

case "${1:-} ${2:-}" in
    "build "*) build "${2:?old tag}" "${3:?new tag}" ;;
    "apt install") apt_install "${3:?old tag}" "${4:?new tag}" ;;
    "apt refused") apt_refused "${3:?new tag}" ;;
    "dnf install") dnf_install "${3:?old tag}" "${4:?new tag}" ;;
    "dnf refused") dnf_refused "${3:?new tag}" ;;
    *) echo "usage: scripts/ci-package-repo.sh build <old> <new> | apt|dnf install <old> <new> | apt|dnf refused <new>" >&2; exit 2 ;;
esac
