#!/usr/bin/env bash
#
# The two helpers release.yml calls (RC-100-32): `scripts/release-assets.sh sums <dir>` writes
# SHA256SUMS over every file but SHA256SUMS*, sorted, which `sha256sum --check` accepts, and
# refuses an empty or missing directory; `sbom <dir> <version>` (RC-110-73) in a scratch tree with a
# stand-in cargo: the application's SBOM under the release's name, the plugins' (those with a
# manifest) in one reproducible archive, the lock file checked, nothing left in the tree, a
# failing or missing SBOM refused; `scripts/release-publish.sh update-manifest <dir> <tag>`
# without the key warns and builds nothing - and fails in degoya/rCommander, as `require-key` does
# before anything is built (RC-110-32) -, with it calls `rc-pack update manifest build` and
# `verify` for the tag's channel and download base, hands the key over in a file only the user can
# read that is gone afterwards, never prints it, and refuses a tag that is no release; the
# manifest's `--schema-change` is `false` only when the migrations are those of the release before
# the tag (a scratch repository with two tags). rc-pack is a stand-in that records its arguments.
#
#   scripts/tests/release-assets.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
ASSETS="$ROOT/scripts/release-assets.sh"
PUBLISH="$ROOT/scripts/release-publish.sh"
unset RCOMMANDER_UPDATE_SIGNING_KEY GITHUB_REPOSITORY

# --- sums --------------------------------------------------------------------------------------------
dir="$SCRATCH/assets"
mkdir -p "$dir/sub"
echo windows > "$dir/rcommander-9.9.9-windows-x64.zip"
echo linux > "$dir/rcommander-9.9.9-linux-x64.tar.gz"
echo old > "$dir/SHA256SUMS.sig"
echo nested > "$dir/sub/ignored"
run_status "$ASSETS" sums "$dir"
expect_status "sums" 0
expect_output "counting the files" "SHA256SUMS lists 2 file(s)"
expect "sorted by name, without SHA256SUMS* or subdirectories" \
    "rcommander-9.9.9-linux-x64.tar.gz rcommander-9.9.9-windows-x64.zip" \
    "$(awk '{ printf "%s%s", sep, $2; sep = " " }' "$dir/SHA256SUMS")"
expect_true "sha256sum --check accepts it" '(cd "$dir" && sha256sum --check --quiet SHA256SUMS)'
mkdir -p "$SCRATCH/empty"
run_status "$ASSETS" sums "$SCRATCH/empty"
expect_status "an empty directory: refused" 1
expect_true "leaving no SHA256SUMS" '[[ ! -e "$SCRATCH/empty/SHA256SUMS" ]]'
run_status "$ASSETS" sums "$SCRATCH/missing"
expect_status "a missing directory: usage error" 2
run_status "$ASSETS" split "$dir" "$SCRATCH/plugins"
expect_status "no other subcommand" 2

# --- sbom (RC-110-73) ----------------------------------------------------------------------------------
# A scratch tree with the script, the application, a library crate and three plugin folders, one
# without a manifest; `cargo` is a stand-in that records its calls and writes an SBOM beside every
# member's Cargo.toml, as cargo-cyclonedx does.
sbom_tree="$SCRATCH/sbom-tree"
mkdir -p "$sbom_tree/scripts" "$sbom_tree/crates/rcommander" "$sbom_tree/crates/rc-db" \
    "$sbom_tree/plugins/b-viewer" "$sbom_tree/plugins/a-viewer" "$sbom_tree/plugins/no-manifest" "$SCRATCH/sbom-bin"
cp "$ASSETS" "$sbom_tree/scripts/release-assets.sh"
for member in crates/rcommander crates/rc-db plugins/b-viewer plugins/a-viewer plugins/no-manifest; do
    touch "$sbom_tree/$member/Cargo.toml"
done
touch "$sbom_tree/plugins/b-viewer/manifest.toml" "$sbom_tree/plugins/a-viewer/manifest.toml"
cat > "$SCRATCH/sbom-bin/cargo" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_CARGO_CALLS"
[[ "$1" == cyclonedx ]] || exit 0
[[ -z "${FAKE_CYCLONEDX_FAIL:-}" ]] || exit 3
for toml in crates/*/Cargo.toml plugins/*/Cargo.toml; do
    member="$(basename "$(dirname "$toml")")"
    [[ "$member" != "${FAKE_CYCLONEDX_SKIP:-}" ]] || continue
    printf '{"bomFormat":"CycloneDX","specVersion":"1.5","name":"%s","epoch":"%s"}\n' \
        "$member" "$SOURCE_DATE_EPOCH" > "$(dirname "$toml")/$member.cdx.json"
done
EOF
chmod +x "$SCRATCH/sbom-bin/cargo"
export FAKE_CARGO_CALLS="$SCRATCH/cargo-calls"
leftovers() { find "$sbom_tree/crates" "$sbom_tree/plugins" -name '*.cdx.json' | wc -l; }
sbom_run() { PATH="$SCRATCH/sbom-bin:$PATH" SOURCE_DATE_EPOCH=1700000000 run_status "$sbom_tree/scripts/release-assets.sh" sbom "$@"; }

out="$SCRATCH/sbom-out"
mkdir -p "$out"
sbom_run "$out" 9.9.9
expect_status "sbom" 0
expect_output "naming what it wrote" "rcommander-9.9.9.cdx.json and rcommander-9.9.9-plugins.cdx.tar.gz (2 plugin(s))"
expect "cargo checks the lock file, then cargo-cyclonedx writes JSON 1.5 for every target" \
    "metadata --locked --format-version 1|cyclonedx --format json --target all --spec-version 1.5" \
    "$(paste -sd '|' "$FAKE_CARGO_CALLS")"
expect "the application's SBOM under the release's name, at SOURCE_DATE_EPOCH" \
    '{"bomFormat":"CycloneDX","specVersion":"1.5","name":"rcommander","epoch":"1700000000"}' \
    "$(cat "$out/rcommander-9.9.9.cdx.json")"
expect "the archive holds the plugins with a manifest, sorted, and nothing else" \
    "a-viewer.cdx.json b-viewer.cdx.json" \
    "$(tar --list --gzip --file "$out/rcommander-9.9.9-plugins.cdx.tar.gz" | paste -sd ' ')"
expect "owned by 0 at SOURCE_DATE_EPOCH" "0/0 2023-11-14" \
    "$(TZ=UTC tar --list --verbose --numeric-owner --gzip --file "$out/rcommander-9.9.9-plugins.cdx.tar.gz" \
        | awk 'NR == 1 { print $2, $4 }')"
expect "no SBOM left beside a member's Cargo.toml" 0 "$(leftovers)"
first="$(sha256sum < "$out/rcommander-9.9.9-plugins.cdx.tar.gz")"
sbom_run "$out" 9.9.9
expect "a second run writes the same archive" "$first" "$(sha256sum < "$out/rcommander-9.9.9-plugins.cdx.tar.gz")"
run_status "$ASSETS" sums "$out"
expect "and SHA256SUMS lists both" "rcommander-9.9.9-plugins.cdx.tar.gz rcommander-9.9.9.cdx.json" \
    "$(awk '{ printf "%s%s", sep, $2; sep = " " }' "$out/SHA256SUMS")"

touch "$sbom_tree/crates/rc-db/stale.cdx.json"
FAKE_CYCLONEDX_FAIL=1 sbom_run "$SCRATCH/empty" 9.9.9
expect_status "cargo-cyclonedx failing: a failure" 1
expect "a stale SBOM removed before, nothing left after" 0 "$(leftovers)"
FAKE_CYCLONEDX_SKIP=rcommander sbom_run "$SCRATCH/empty" 9.9.9
expect_status "no application SBOM: a failure" 1
expect_output "naming it" "cargo-cyclonedx wrote no $sbom_tree/crates/rcommander/rcommander.cdx.json"
FAKE_CYCLONEDX_SKIP=b-viewer sbom_run "$SCRATCH/empty" 9.9.9
expect_status "a plugin without its SBOM: a failure" 1
expect "and nothing written" "" "$(find "$SCRATCH/empty" -type f)"
expect "nor left in the tree" 0 "$(leftovers)"
sbom_run "$SCRATCH/empty" 9.9
expect_status "no version: usage error" 2
sbom_run "$SCRATCH/missing" 9.9.9
expect_status "a missing directory: usage error" 2
sbom_run "$out"
expect_status "without a version: usage error" 2

# --- update-manifest -----------------------------------------------------------------------------------
mkdir -p "$SCRATCH/bin"
cat > "$SCRATCH/bin/rc-pack" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_PACK_CALLS"
if [[ "$3" == build ]]; then
    key="" out=""
    while [[ $# -gt 0 ]]; do
        case "$1" in --key) key="$2" ;; --out) out="$2" ;; esac
        shift
    done
    stat -c %a "$key" > "$FAKE_PACK_CALLS.mode"
    cat "$key" > "$FAKE_PACK_CALLS.key"
    printf '%s\n' "$key" > "$FAKE_PACK_CALLS.path"
    [[ -z "${FAKE_PACK_FAIL:-}" ]] || exit 4
    echo '{"signed": true}' > "$out"
fi
EOF
chmod +x "$SCRATCH/bin/rc-pack"
export PATH="$SCRATCH/bin:$PATH" FAKE_PACK_CALLS="$SCRATCH/pack-calls"

run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "without the key: no failure" 0
expect_output "but a warning" "::warning::RCOMMANDER_UPDATE_SIGNING_KEY is not set"
expect_true "no manifest" '[[ ! -e "$dir/rcommander-update-stable.json" ]]'
expect_true "and rc-pack not called" '[[ ! -e "$FAKE_PACK_CALLS" ]]'
GITHUB_REPOSITORY=someone/fork run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "without the key in a fork: no failure either" 0
GITHUB_REPOSITORY=degoya/rCommander run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "without the key in the release repository: a failure" 1
expect_output "as an error" "::error::RCOMMANDER_UPDATE_SIGNING_KEY is not set"
expect_true "no manifest" '[[ ! -e "$dir/rcommander-update-stable.json" ]]'
expect_true "and rc-pack not called" '[[ ! -e "$FAKE_PACK_CALLS" ]]'
GITHUB_REPOSITORY=degoya/rCommander run_status "$PUBLISH" require-key
expect_status "require-key without the key in the release repository" 1
expect_output "is the same error" "::error::RCOMMANDER_UPDATE_SIGNING_KEY is not set"
GITHUB_REPOSITORY=someone/fork run_status "$PUBLISH" require-key
expect_status "require-key without the key in a fork" 0
expect_output "warns" "::warning::RCOMMANDER_UPDATE_SIGNING_KEY is not set"
run_status "$PUBLISH" require-key
expect_status "require-key without the key in a local run" 0
GITHUB_REPOSITORY=degoya/rCommander RCOMMANDER_UPDATE_SIGNING_KEY=made-up run_status "$PUBLISH" require-key
expect_status "require-key with the key" 0
expect "says nothing" "" "$output"

material="made-up-key-material"
RCOMMANDER_UPDATE_SIGNING_KEY="$material" run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "with the key: the stable manifest" 0
expect_true "is written" '[[ -s "$dir/rcommander-update-stable.json" ]]'
expect "built for the stable channel and the tag's release, with its notes" \
    "update manifest build --channel stable --version 9.9.9 --assets $dir --base-url https://github.com/degoya/rCommander/releases/download/v9.9.9/ --key $(cat "$FAKE_PACK_CALLS.path") --notes $ROOT/RELEASE-NOTES.md --schema-change true --out $dir/rcommander-update-stable.json" \
    "$(sed -n 1p "$FAKE_PACK_CALLS")"
expect "then verified" "update manifest verify $dir/rcommander-update-stable.json --assets $dir" \
    "$(sed -n 2p "$FAKE_PACK_CALLS")"
expect "the key reached rc-pack" "$material" "$(cat "$FAKE_PACK_CALLS.key")"
expect "in a file only the user can read" 600 "$(cat "$FAKE_PACK_CALLS.mode")"
expect_true "which is gone afterwards" '[[ ! -e "$(cat "$FAKE_PACK_CALLS.path")" ]]'
expect_true "and the key is in no output" '! grep -qF "$material" <<< "$output"'

rm -f "$FAKE_PACK_CALLS"
echo "$material" > "$SCRATCH/update.key"
GITHUB_REPOSITORY=someone/fork RCOMMANDER_UPDATE_SIGNING_KEY="$SCRATCH/update.key" \
    run_status "$PUBLISH" update-manifest "$dir" v9.9.9-beta.2
expect_status "a beta tag with the key as a path" 0
expect_true "is the beta manifest" '[[ -s "$dir/rcommander-update-beta.json" ]]'
expect_true "on the beta channel, from the repository's releases, with the key file as it is" \
    'grep -qF -- "--channel beta --version 9.9.9-beta.2 --assets $dir --base-url https://github.com/someone/fork/releases/download/v9.9.9-beta.2/ --key $SCRATCH/update.key" "$FAKE_PACK_CALLS"'
expect_true "which stays" '[[ -f "$SCRATCH/update.key" ]]'

FAKE_PACK_FAIL=1 RCOMMANDER_UPDATE_SIGNING_KEY="$material" run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "a failing rc-pack fails the step" 4
for tag in 9.9.9 v9.9 v9.9.9-rc.1; do
    RCOMMANDER_UPDATE_SIGNING_KEY="$material" run_status "$PUBLISH" update-manifest "$dir" "$tag"
    expect_status "'$tag' is no release tag" 2
done
RC_PACK="$SCRATCH/no-rc-pack" RCOMMANDER_UPDATE_SIGNING_KEY="$material" run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "with the key but without rc-pack: a failure" 1
run_status "$PUBLISH" update-manifest "$dir"
expect_status "a call without the tag: usage" 2

# --- schema_change (RC-110-32) -------------------------------------------------------------------------
# The script in a scratch repository with the releases v9.9.7 ... v9.9.9: v9.9.8 keeps the
# migrations of v9.9.7, v9.9.9 adds one.
tree="$SCRATCH/tree"
mkdir -p "$tree/scripts/lib" "$tree/crates/rc-db/migrations"
cp "$ROOT/scripts/release-publish.sh" "$ROOT/scripts/update-schema-change.sh" "$tree/scripts/"
cp "$ROOT/scripts/lib/release-tag.sh" "$ROOT/scripts/lib/changelog.sh" "$tree/scripts/lib/"
git_quiet() { git -C "$tree" -c user.name=test -c user.email=test@example.invalid -c init.defaultBranch=main "$@"; }
git_quiet init -q
echo "create table one (id integer);" > "$tree/crates/rc-db/migrations/0001_one.sql"
git_quiet add -A && git_quiet commit -qm 9.9.7 && git_quiet tag v9.9.7
echo notes > "$tree/RELEASE-NOTES.md"
git_quiet add -A && git_quiet commit -qm 9.9.8 && git_quiet tag v9.9.8
echo "create table two (id integer);" > "$tree/crates/rc-db/migrations/0002_two.sql"
git_quiet add -A && git_quiet commit -qm 9.9.9 && git_quiet tag v9.9.9
schema_of() {
    rm -f "$FAKE_PACK_CALLS"
    RCOMMANDER_UPDATE_SIGNING_KEY="$material" run_status "$tree/scripts/release-publish.sh" update-manifest "$dir" "$1"
    sed -n 's/.* --schema-change \([^ ]*\) .*/\1/p' "$FAKE_PACK_CALLS"
}
expect "the migrations of the release before: no schema change" false "$(schema_of v9.9.8)"
expect "a new migration: a schema change" true "$(schema_of v9.9.9)"
expect "no release before the tag: a schema change" true "$(schema_of v9.9.7)"
git_quiet tag -d v9.9.7 > /dev/null
expect "the earlier tags not fetched (a shallow checkout): a schema change" true "$(schema_of v9.9.8)"
git_quiet tag -d v9.9.8 > /dev/null
expect "the tag itself not in the repository: a schema change" true "$(schema_of v9.9.8)"

# --- release-text (RC-110-33) --------------------------------------------------------------------------
printf '%s\n' '# Release notes' '' '## 9.9.9' '' '- A point.' '- Another.' '' '## 9.9.8' '' '- Older.' \
    '' '## 9.9.7-beta.1' '' '- The beta alone.' '' '## 9.9.6' '' > "$tree/RELEASE-NOTES.md"
text="$SCRATCH/release-text.md"
run_status "$tree/scripts/release-publish.sh" release-text v9.9.9 "$text"
expect_status "release-text of a version with a section" 0
expect "is its points, without the heading and the next section" "- A point.|- Another." \
    "$(sed '/^$/d' "$text" | paste -sd '|')"
run_status "$tree/scripts/release-publish.sh" release-text v9.9.8-beta.2 "$text"
expect "a beta without a section of its own takes its release's" "0 - Older." "$status $(sed '/^$/d' "$text")"
run_status "$tree/scripts/release-publish.sh" release-text v9.9.7-beta.1 "$text"
expect "a beta's own section first" "0 - The beta alone." "$status $(sed '/^$/d' "$text")"
run_status "$tree/scripts/release-publish.sh" release-text v9.9.6 "$text"
expect_status "a section without text: a failure" 1
expect_output "named for the workflow" "::error::RELEASE-NOTES.md has no section ## 9.9.6"
run_status "$tree/scripts/release-publish.sh" release-text v9.9.5 "$text"
expect_status "no section: a failure" 1
run_status "$tree/scripts/release-publish.sh" release-text 9.9 "$text"
expect_status "no release tag: a failure" 1

finish_tests release-assets
