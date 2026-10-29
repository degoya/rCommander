#!/usr/bin/env bash
#
# The two helpers release.yml calls (RC-100-32): `scripts/release-assets.sh sums <dir>` writes
# SHA256SUMS over every file but SHA256SUMS*, sorted, which `sha256sum --check` accepts, and
# refuses an empty or missing directory; `scripts/release-publish.sh update-manifest <dir> <tag>`
# without the key warns and builds nothing, with it calls `rc-pack update manifest build` and
# `verify` for the tag's channel and download base, hands the key over in a file only the user can
# read that is gone afterwards, never prints it, and refuses a tag that is no release. rc-pack is a
# stand-in that records its arguments.
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

material="made-up-key-material"
RCOMMANDER_UPDATE_SIGNING_KEY="$material" run_status "$PUBLISH" update-manifest "$dir" v9.9.9
expect_status "with the key: the stable manifest" 0
expect_true "is written" '[[ -s "$dir/rcommander-update-stable.json" ]]'
expect "built for the stable channel and the tag's release" \
    "update manifest build --channel stable --version 9.9.9 --assets $dir --base-url https://github.com/degoya/rCommander/releases/download/v9.9.9/ --key $(cat "$FAKE_PACK_CALLS.path") --out $dir/rcommander-update-stable.json" \
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

finish_tests release-assets
