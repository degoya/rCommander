#!/usr/bin/env bash
#
# scripts/package-repo.sh on fixture packages and a throwaway GPG key with a passphrase
# (RC-100-54, after rDownloader's scripts/tests/package-repo.sh): every package lands under the
# name its metadata gives it, each architecture's index lists its own, the Release is signed twice
# and verifies with gpgv against the exported key, a re-run changes nothing, newer releases keep
# the newest --keep versions, a wrong passphrase signs nothing, and the rendered .sources and .repo
# point at the base URL. The fixtures are built here with dpkg-deb and, where rpmbuild, rpmsign
# and createrepo_c are installed, with rpmbuild; a half without its tools is skipped with a line
# saying so (packages-repo.yml installs them and runs both; its install jobs take real packages
# built from two releases' tarballs, scripts/ci-package-repo.sh).
#
#   scripts/tests/package-repo.sh
set -euo pipefail
shopt -s inherit_errexit

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
cleanup() {
    gpgconf --homedir "$SCRATCH/gnupg" --kill all 2> /dev/null || true
    gpgconf --homedir "$SCRATCH/empty-gnupg" --kill all 2> /dev/null || true
    rm -rf "$SCRATCH"
}
trap cleanup EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

for tool in gpg gpgv dpkg-deb apt-ftparchive; do
    if ! command -v "$tool" > /dev/null; then
        echo "skip package-repo.sh: $tool is not installed"
        exit 0
    fi
done
with_rpm=1
for tool in rpmbuild rpmsign rpmkeys createrepo_c; do
    if ! command -v "$tool" > /dev/null; then
        echo "skip the rpm half: $tool is not installed"
        with_rpm=0
        break
    fi
done

export GNUPGHOME="$SCRATCH/gnupg"
install -d -m 0700 "$GNUPGHOME"
printf '%s\n' 'a test passphrase' > "$SCRATCH/passphrase"
gpg --batch --quiet --pinentry-mode loopback --passphrase-file "$SCRATCH/passphrase" \
    --quick-gen-key 'rCommander test <test@example.invalid>' ed25519 sign 3y
fingerprint="$(gpg --batch --with-colons --list-secret-keys 2> /dev/null | awk -F: '$1 == "fpr" { print $10; exit }')"

make_deb() {
    local version="$1" arch="$2" dir="$SCRATCH/build/deb-$1-$2"
    mkdir -p "$dir/DEBIAN" "$dir/usr/lib/rcommander-fixture"
    echo "$version" > "$dir/usr/lib/rcommander-fixture/version"
    cat > "$dir/DEBIAN/control" << EOF
Package: rcommander
Version: $version-1
Architecture: $arch
Maintainer: rCommander test <test@example.invalid>
Description: rCommander repository fixture
EOF
    # Release asset names, not the pool's: the script renames by the package's metadata.
    dpkg-deb --root-owner-group --build "$dir" "$3/rcommander-$version-$arch.deb" > /dev/null
}

make_rpm() {
    local version="$1" arch="$2" top="$SCRATCH/build/rpm"
    mkdir -p "$top/SPECS" "$SCRATCH/rpmdb"
    cat > "$top/SPECS/fixture.spec" << 'EOF'
Name: rcommander
Version: %{fixture_version}
Release: 1
Summary: rCommander repository fixture
License: MIT
%description
rCommander repository fixture
%install
mkdir -p %{buildroot}/usr/lib/rcommander-fixture
echo %{version} > %{buildroot}/usr/lib/rcommander-fixture/version
%files
/usr/lib/rcommander-fixture/version
EOF
    rpmbuild --quiet -bb --target "$arch" --define "_topdir $top" --define "_dbpath $SCRATCH/rpmdb" \
        --define "fixture_version $version" --define "debug_package %{nil}" \
        --define "__os_install_post %{nil}" "$top/SPECS/fixture.spec" > /dev/null 2>&1
    cp "$top/RPMS/$arch/rcommander-$version-1.$arch.rpm" "$3/rcommander-$version.$arch.rpm"
}

incoming() {
    local dir="$SCRATCH/incoming-$1" version
    mkdir -p "$dir"
    shift
    for version in "$@"; do
        make_deb "$version" amd64 "$dir"
        make_deb "$version" arm64 "$dir"
        if [[ "$with_rpm" -eq 1 ]]; then
            make_rpm "$version" x86_64 "$dir"
            make_rpm "$version" aarch64 "$dir"
        fi
    done
    echo "$dir"
}

repo() { "$ROOT/scripts/package-repo.sh" "$@" --passphrase-file "$SCRATCH/passphrase"; }
site="$SCRATCH/site"
dists="$site/deb/dists/stable"
# The versions one architecture's Packages index lists, in order.
listed() { awk '/^Version: / { print $2 }' "$dists/main/binary-$1/Packages" | sort -V | tr '\n' ' ' | sed 's/ $//'; }
# gpgv against the key the script exported, so the published key is what verifies.
verify() {
    gpg --batch --yes --dearmor --output "$SCRATCH/published.gpg" "$site/rcommander-packages.asc"
    gpgv --keyring "$SCRATCH/published.gpg" "$@" > /dev/null 2>&1
}

first="$(incoming first 1.0.0)"
run_status repo "$first" "$site" --keep 2 --base-url file:///repo/
expect_status "a first release builds the repository" 0
expect_output "and names the key it signed with" "key $fingerprint"
expect_true "the amd64 package under its pool name" "[[ -f '$site/deb/pool/main/r/rcommander/rcommander_1.0.0-1_amd64.deb' ]]"
expect_true "the arm64 package under its pool name" "[[ -f '$site/deb/pool/main/r/rcommander/rcommander_1.0.0-1_arm64.deb' ]]"
expect "amd64's index lists its version" "1.0.0-1" "$(listed amd64)"
expect "and only amd64 packages" "amd64" "$(awk '/^Architecture: / { print $2 }' "$dists/main/binary-amd64/Packages" | sort -u)"
expect "arm64's index lists its own" "arm64" "$(awk '/^Architecture: / { print $2 }' "$dists/main/binary-arm64/Packages" | sort -u)"
expect "the index points into the pool" "pool/main/r/rcommander/rcommander_1.0.0-1_amd64.deb" \
    "$(awk '/^Filename: / { print $2 }' "$dists/main/binary-amd64/Packages")"
expect "Packages.gz is the same index" "" "$(gzip -dc "$dists/main/binary-amd64/Packages.gz" | diff - "$dists/main/binary-amd64/Packages")"
expect "the Release names suite, component and architectures" "stable|main|amd64 arm64" \
    "$(awk -F': ' '$1 == "Suite" { s = $2 } $1 == "Components" { c = $2 } $1 == "Architectures" { a = $2 }
        END { print s "|" c "|" a }' "$dists/Release")"
expect "the Release carries the index's SHA256" "1" \
    "$(awk -v sum="$(sha256sum < "$dists/main/binary-amd64/Packages" | cut -d' ' -f1)" \
        '$1 == sum && $3 == "main/binary-amd64/Packages" { n++ } END { print n + 0 }' "$dists/Release")"
run_status verify "$dists/InRelease"
expect_status "InRelease verifies with the published key" 0
run_status verify "$dists/Release.gpg" "$dists/Release"
expect_status "Release.gpg verifies the Release" 0
expect "InRelease signs the Release's text" "" \
    "$(gpg --batch --quiet --decrypt "$dists/InRelease" 2> /dev/null | diff - "$dists/Release")"
cp "$dists/Release" "$SCRATCH/Release.tampered"
echo "Label: someone else" >> "$SCRATCH/Release.tampered"
run_status verify "$dists/Release.gpg" "$SCRATCH/Release.tampered"
expect_status "a changed Release no longer verifies" 1

sources="$site/rcommander.sources"
expect "the .sources points at the deb half of the base URL, without its trailing slash" \
    "URIs: file:///repo/deb" "$(grep '^URIs: ' "$sources")"
expect "and trusts the key file, never trusted=yes" "Signed-By: /etc/apt/keyrings/rcommander-packages.asc" \
    "$(grep '^Signed-By: ' "$sources")"
expect "no trusted=yes" "0" "$(grep -ci 'trusted=' "$sources" || true)"
expect "the README names the fingerprint" "1" "$(grep -c "\`$fingerprint\`" "$site/README.md")"
expect "and the kept versions" "1" "$(grep -c '^2 versions of each package' "$site/README.md")"
expect "no placeholder is left" "0" "$(cat "$site"/README.md "$site"/index.html "$sources" | grep -c '@[A-Z0-9_]*@' || true)"
expect_true "Pages serves the tree as it is" "[[ -f '$site/.nojekyll' && -f '$site/index.html' ]]"

if [[ "$with_rpm" -eq 1 ]]; then
    expect_true "the rpms under their metadata names" \
        "[[ -f '$site/rpm/packages/rcommander-1.0.0-1.x86_64.rpm' && -f '$site/rpm/packages/rcommander-1.0.0-1.aarch64.rpm' ]]"
    run_status verify "$site/rpm/repodata/repomd.xml.asc" "$site/rpm/repodata/repomd.xml"
    expect_status "repomd.xml.asc verifies with the published key" 0
    rpmkeys --dbpath "$SCRATCH/rpmdb" --import "$site/rcommander-packages.asc"
    run_status rpmkeys --dbpath "$SCRATCH/rpmdb" --checksig "$site/rpm/packages/rcommander-1.0.0-1.x86_64.rpm"
    expect_output "the rpm itself is signed with the key" "signatures OK"
    expect "the .repo points at the rpm half" "baseurl=file:///repo/rpm" "$(grep '^baseurl=' "$site/rcommander.repo")"
    expect "and checks packages and metadata, failing hard instead of skipping the repository" \
        "gpgcheck=1 repo_gpgcheck=1 skip_if_unavailable=0" \
        "$(grep -E '^((repo_)?gpgcheck|skip_if_unavailable)=' "$site/rcommander.repo" | tr '\n' ' ' | sed 's/ $//')"
else
    expect_true "without the rpm half no .repo is written" "[[ ! -e '$site/rcommander.repo' ]]"
fi

cp "$dists/InRelease" "$SCRATCH/InRelease.first"
run_status repo "$first" "$site" --keep 2 --base-url file:///repo/
expect_status "the same release again" 0
expect_output "keeps the published file" "kept    deb/pool/main/r/rcommander/rcommander_1.0.0-1_amd64.deb"
expect_output "and says the index stays" "deb: nothing added or removed"
expect "InRelease is untouched" "" "$(cmp "$SCRATCH/InRelease.first" "$dists/InRelease" 2>&1)"

second="$(incoming second 1.1.0 1.2.0 1.3.0)"
# From here on the default --keep, which is 3.
run_status repo "$second" "$site" --base-url file:///repo/
expect_status "three newer releases" 0
expect_output "push the oldest out" "removed deb/pool/main/r/rcommander/rcommander_1.0.0-1_amd64.deb (beyond the newest 3)"
expect "amd64 keeps the newest three" "1.1.0-1 1.2.0-1 1.3.0-1" "$(listed amd64)"
expect "arm64 keeps the newest three" "1.1.0-1 1.2.0-1 1.3.0-1" "$(listed arm64)"
expect "the pool holds six packages" "6" "$(find "$site/deb/pool" -name '*.deb' | wc -l | tr -d ' ')"
run_status verify "$dists/InRelease"
expect_status "the new InRelease verifies" 0
if [[ "$with_rpm" -eq 1 ]]; then
    expect "the rpm half keeps the newest three per architecture" \
        "rcommander-1.1.0-1.aarch64.rpm rcommander-1.1.0-1.x86_64.rpm rcommander-1.2.0-1.aarch64.rpm rcommander-1.2.0-1.x86_64.rpm rcommander-1.3.0-1.aarch64.rpm rcommander-1.3.0-1.x86_64.rpm" \
        "$(cd "$site/rpm/packages" && printf '%s\n' *.rpm | sort | tr '\n' ' ' | sed 's/ $//')"
    run_status verify "$site/rpm/repodata/repomd.xml.asc" "$site/rpm/repodata/repomd.xml"
    expect_status "the new repomd.xml.asc verifies" 0
fi

run_status repo "$second" "$site" --refresh
expect_status "--refresh signs again with nothing new" 0
expect_output "and says so" "signed  deb/dists/stable/InRelease"

mkdir -p "$SCRATCH/nothing"
run_status repo "$SCRATCH/nothing" "$SCRATCH/no-site"
expect_status "no packages and no repository is refused" 1
export GNUPGHOME="$SCRATCH/empty-gnupg"
install -d -m 0700 "$GNUPGHOME"
run_status repo "$first" "$SCRATCH/keyless"
expect_status "no secret key is refused" 1
expect_output "naming the count" "0 secret keys match"
export GNUPGHOME="$SCRATCH/gnupg"
run_status repo "$first" "$SCRATCH/other" --key nobody@example.invalid
expect_status "a --key that matches nothing is refused" 1
run_status repo "$first" "$SCRATCH/other" --keep 0
expect_status "--keep 0 is refused" 1
run_status "$ROOT/scripts/package-repo.sh" "$first" "$SCRATCH/other" --passphrase-file "$SCRATCH/missing"
expect_status "a missing passphrase file is refused" 1
# The agent forgets the passphrase it was given, so the wrong one is really asked for.
gpgconf --kill gpg-agent
mv "$SCRATCH/passphrase" "$SCRATCH/passphrase.real"
printf '%s\n' 'a wrong passphrase' > "$SCRATCH/passphrase"
run_status repo "$first" "$SCRATCH/wrong-passphrase"
expect_true "a wrong passphrase signs nothing" "[[ $status -ne 0 && ! -e '$SCRATCH/wrong-passphrase/deb/dists/stable/InRelease' ]]"
mv "$SCRATCH/passphrase.real" "$SCRATCH/passphrase"
run_status repo "$first" "$SCRATCH/other" --base-url ftp://example.invalid
expect_status "a base URL that is not http(s) or file is refused" 1
run_status repo "$SCRATCH/missing" "$SCRATCH/other"
expect_status "a missing incoming directory is refused" 1
run_status repo "$first"
expect_status "too few arguments is a usage error" 2
run_status repo "$first" "$SCRATCH/other" --unknown
expect_status "an unknown option is a usage error" 2

# The public half of the release key in the tree, which release-repositories.sh signs with only.
published="$ROOT/packaging/repository/rcommander-packages.asc"
expect "the tree's key is a public key block" "-----BEGIN PGP PUBLIC KEY BLOCK-----" "$(head -n 1 "$published")"
expect "and holds no secret part" "0" "$(grep -c 'PRIVATE KEY' "$published" || true)"
expect "the key rcommander-packages" "56CCFFDA649CC1654735885048F16D81F44CB87E" \
    "$(gpg --batch --with-colons --show-keys "$published" 2> /dev/null | awk -F: '$1 == "fpr" { print $10; exit }')"

finish_tests "package-repo.sh"
