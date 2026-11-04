#!/usr/bin/env bash
#
# scripts/package-managers.sh against a SHA256SUMS in the release's format (RC-100-52,
# RC-100-53): every archive's hash lands beside its own URL in the cask and the formula, the Scoop
# manifest is JSON that unpacks the zip's version folder and keeps no persist, the three winget
# manifests name the portable zip, its upper-case hash and the program in the version folder,
# the READMEs name tap and bucket; a release that lacks an archive renders nothing; `--check`
# renders against the last release's SHA256SUMS (here a file, no network) and fails when the
# templates name an archive it does not have.
#
#   scripts/tests/package-managers.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

render() { "$ROOT/scripts/package-managers.sh" "$@"; }
# 64 times the character $1.
hash_of() { printf "$1%.0s" {1..64}; }
# The line after the one naming $2 in file $1: the sha256 under a url.
line_after() { awk -v needle="$2" 'found { print; exit } index($0, needle) { found = 1 }' "$1"; }
# One field of a JSON file, by a Python expression over `m`.
field() { python3 -I -c "import json, sys; m = json.load(open(sys.argv[1])); print($2)" "$1"; }

# The release's SHA256SUMS for version $1, as scripts/release-assets.sh writes it.
sums_for() {
    printf '%s  ./%s\n' \
        "$(hash_of a)" "rcommander-$1-linux-arm64.tar.gz" \
        "$(hash_of b)" "rcommander-$1-linux-x64.tar.gz" \
        "$(hash_of c)" "rcommander-$1-macos-aarch64.zip" \
        "$(hash_of d)" "rcommander-$1-macos-x86_64.zip" \
        "$(hash_of e)" "rcommander-$1-windows-x64.zip"
}
SUMS="$SCRATCH/SHA256SUMS"
sums_for 1.6.0 > "$SUMS"

run_status render v1.6.0 "$SUMS" "$SCRATCH/out"
expect_status "renders every file from the release's SHA256SUMS" 0
out="$SCRATCH/out"
base="https://github.com/degoya/rCommander/releases/download/v1.6.0"
expect "the files as each repository holds them" \
    "homebrew/Casks/rcommander.rb homebrew/Formula/rcommander.rb homebrew/README.md scoop/README.md scoop/bucket/rcommander.json winget/degoya.rCommander.installer.yaml winget/degoya.rCommander.locale.en-US.yaml winget/degoya.rCommander.yaml" \
    "$(cd "$out" && find . -type f | sed 's#^\./##' | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')"
expect "no placeholder is left anywhere" "0" "$(cat "$out"/homebrew/*/* "$out"/scoop/bucket/* "$out"/winget/* "$out"/*/README.md | grep -c '@[A-Z0-9_]*@' || true)"

# Homebrew: the cask for macOS, the formula for Linux.
cask="$out/homebrew/Casks/rcommander.rb"
formula="$out/homebrew/Formula/rcommander.rb"
expect "the cask's version, without the tag's v" '  version "1.6.0"' "$(grep '^  version ' "$cask")"
expect "cask, Apple Silicon: its archive and its hash" "    sha256 \"$(hash_of c)\"" \
    "$(line_after "$cask" "url \"$base/rcommander-1.6.0-macos-aarch64.zip\"")"
expect "cask, Intel: its archive and its hash" "    sha256 \"$(hash_of d)\"" \
    "$(line_after "$cask" "url \"$base/rcommander-1.6.0-macos-x86_64.zip\"")"
expect "the cask installs the bundle and its command" \
    'app "rCommander.app" binary "#{appdir}/rCommander.app/Contents/MacOS/rcommander"' \
    "$(grep -E '^  (app|binary) ' "$cask" | sed 's/^  //' | tr '\n' ' ' | sed 's/ $//')"
expect "the cask explains Gatekeeper" "1" "$(grep -c 'signed ad hoc, not notarised' "$cask")"
expect "formula, Linux arm64: its archive and its hash" "      sha256 \"$(hash_of a)\"" \
    "$(line_after "$formula" "url \"$base/rcommander-1.6.0-linux-arm64.tar.gz\"")"
expect "formula, Linux x64: its archive and its hash" "      sha256 \"$(hash_of b)\"" \
    "$(line_after "$formula" "url \"$base/rcommander-1.6.0-linux-x64.tar.gz\"")"
expect "the formula is Linux only" "1" "$(grep -c '^  depends_on :linux$' "$formula")"
expect "the formula names its tap" "1" "$(grep -c 'in the tap degoya/homebrew-rcommander' "$formula")"
if command -v ruby > /dev/null; then
    run_status ruby -c "$formula"
    expect_status "the formula is Ruby" 0
    run_status ruby -c "$cask"
    expect_status "the cask is Ruby" 0
else
    echo "skip the formula and the cask are Ruby: no ruby on this machine"
fi

# Scoop.
scoop="$out/scoop/bucket/rcommander.json"
expect "the manifest is JSON with the version" "1.6.0" "$(field "$scoop" 'm["version"]')"
expect "the manifest's archive" "$base/rcommander-1.6.0-windows-x64.zip" \
    "$(field "$scoop" 'm["architecture"]["64bit"]["url"]')"
expect "the manifest's hash" "$(hash_of e)" "$(field "$scoop" 'm["architecture"]["64bit"]["hash"]')"
expect "it unpacks the zip's version folder" "rcommander-1.6.0-windows-x64" \
    "$(field "$scoop" 'm["architecture"]["64bit"]["extract_dir"]')"
expect "a Start menu entry" "rcommander.exe rCommander" "$(field "$scoop" '" ".join(m["shortcuts"][0])')"
expect "no persist: the data lives in the user's folder" "False" "$(field "$scoop" '"persist" in m')"
expect "checkver asks the repository's releases" "https://github.com/degoya/rCommander" \
    "$(field "$scoop" 'm["checkver"]["github"]')"
expect "autoupdate keeps Scoop's own variables" \
    "https://github.com/degoya/rCommander/releases/download/v\$version/rcommander-\$version-windows-x64.zip rcommander-\$version-windows-x64" \
    "$(field "$scoop" 'm["autoupdate"]["architecture"]["64bit"]["url"] + " " + m["autoupdate"]["architecture"]["64bit"]["extract_dir"]')"

# winget (RC-100-53): three manifests of one version, the portable zip.
winget="$out/winget"
installer="$winget/degoya.rCommander.installer.yaml"
expect "every manifest names the version" "3" "$(cat "$winget"/*.yaml | grep -c '^PackageVersion: 1.6.0$')"
expect "every manifest names the package" "3" \
    "$(cat "$winget"/*.yaml | grep -c '^PackageIdentifier: degoya.rCommander$')"
expect "the installer is the Windows zip" \
    "  InstallerUrl: $base/rcommander-1.6.0-windows-x64.zip" "$(grep '^  InstallerUrl: ' "$installer")"
expect "with its hash in upper case" "  InstallerSha256: $(hash_of E)" "$(grep '^  InstallerSha256: ' "$installer")"
expect "a portable inside a zip" "InstallerType: zip NestedInstallerType: portable" \
    "$(grep -E '^(Nested)?InstallerType: ' "$installer" | tr '\n' ' ' | sed 's/ $//')"
expect "the program in the zip's version folder" \
    '- RelativeFilePath: rcommander-1.6.0-windows-x64\rcommander.exe' "$(grep '^- RelativeFilePath: ' "$installer")"
expect "the command rcommander" "  PortableCommandAlias: rcommander" "$(grep '^  PortableCommandAlias: ' "$installer")"
expect "no Scope on a portable package" "" "$(grep '^Scope: ' "$installer" || true)"
expect "the release notes of the tag" \
    "ReleaseNotesUrl: https://github.com/degoya/rCommander/releases/tag/v1.6.0" \
    "$(grep '^ReleaseNotesUrl: ' "$winget/degoya.rCommander.locale.en-US.yaml")"
if python3 -I -c 'import yaml' 2> /dev/null; then
    expect "the manifests are YAML of one schema version" "1.12.0 1.12.0 1.12.0" \
        "$(python3 -I -c 'import sys, yaml; print(" ".join(yaml.safe_load(open(f))["ManifestVersion"] for f in sys.argv[1:]))' \
            "$winget"/*.yaml)"
else
    echo "skip the manifests are YAML: no PyYAML on this machine"
fi

expect "the tap's README installs cask and formula from the tap" \
    "brew install --cask degoya/rcommander/rcommander brew install degoya/rcommander/rcommander" \
    "$(grep '^brew install' "$out/homebrew/README.md" | tr '\n' ' ' | sed 's/ $//')"
expect "the bucket's README adds the bucket" \
    "scoop bucket add rcommander https://github.com/degoya/scoop-rcommander" \
    "$(grep '^scoop bucket add' "$out/scoop/README.md")"

# A fork and a local fixture.
sums_for 1.6.1-rc.1 > "$SCRATCH/rc-sums"
run_status render 1.6.1-rc.1 "$SCRATCH/rc-sums" "$SCRATCH/fork" --repository someone/rCommander \
    --base-url "file:///tmp/fixture/"
expect_status "a pre-release version, another repository and a base URL" 0
expect "the archives come from the base URL, without its trailing slash" \
    '      url "file:///tmp/fixture/rcommander-1.6.1-rc.1-linux-x64.tar.gz"' \
    "$(grep 'linux-x64' "$SCRATCH/fork/homebrew/Formula/rcommander.rb")"
expect "the fork's bucket" "True" \
    "$(field "$SCRATCH/fork/scoop/bucket/rcommander.json" '"someone/scoop-rcommander" in m["##"]')"

# sha256sum's other spellings: no ./, binary mode's *, upper case.
sed -e 's#  \./rcommander-1.6.0-linux#  rcommander-1.6.0-linux#' -e 's#  \./rcommander-1.6.0-macos# *rcommander-1.6.0-macos#' \
    -e 's#^e\{64\}#'"$(hash_of E)"'#' "$SUMS" > "$SCRATCH/spellings"
run_status render 1.6.0 "$SCRATCH/spellings" "$SCRATCH/spellings-out"
expect_status "names without ./ or with * are found" 0
expect "the same files either way" "" "$(diff -r "$out" "$SCRATCH/spellings-out")"

grep -v 'linux-arm64' "$SUMS" > "$SCRATCH/incomplete"
run_status render 1.6.0 "$SCRATCH/incomplete" "$SCRATCH/incomplete-out"
expect_status "a release without one archive is refused" 1
expect_output "and the archive is named" "no SHA-256 for rcommander-1.6.0-linux-arm64.tar.gz"
expect_true "and nothing is written" '[[ ! -e "$SCRATCH/incomplete-out" ]]'

run_status render 1.6 "$SUMS" "$SCRATCH/bad-version"
expect_status "a version that is not X.Y.Z is refused" 1
run_status render 1.6.0 "$SUMS" "$SCRATCH/bad-repo" --repository degoya
expect_status "a repository that is not OWNER/NAME is refused" 1
run_status render 1.6.0 "$SUMS"
expect_status "too few arguments is a usage error" 2

# --check against the last release's SHA256SUMS.
run_status render --check --sums "$SUMS"
expect_status "--check renders against the last release" 0
expect_output "naming its version" "against the last release, 1.6.0: 8 files"
run_status render --check --sums "$SCRATCH/incomplete"
expect_status "--check fails when a template names an archive the release lacks" 1
expect_output "naming the archive" "no SHA-256 for rcommander-1.6.0-linux-arm64.tar.gz"
grep -v 'windows-x64' "$SUMS" > "$SCRATCH/no-windows"
run_status render --check --sums "$SCRATCH/no-windows"
expect_status "--check without the Windows zip cannot tell the version" 1
run_status render --check --sums "$SCRATCH/none"
expect_status "--check with a missing file is refused" 1
run_status render --check --unknown
expect_status "--check with an unknown option is a usage error" 2

finish_tests "package-managers.sh"
