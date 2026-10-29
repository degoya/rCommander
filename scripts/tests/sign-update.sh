#!/usr/bin/env bash
#
# scripts/sign-update.sh in a scratch repository, without network and with HOME bent into the
# scratch folder: a missing key, a package of another profile or version and the wrong channel
# stop the run before a file is written; a good run writes the manifest. With a built rc-pack
# (RC_PACK, or target/*/rc-pack) the run is real - a throw-away key from `rc-pack keygen`, the
# round trip verified, `--bump` raising the sequence; without one a stand-in records the call and
# the round trip is skipped with a notice (this test builds nothing).
#
#   scripts/tests/sign-update.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
# shellcheck disable=SC2034  # before and after are read by the conditions expect_true evaluates
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
export HOME="$SCRATCH/home"
KEY_DIR="$HOME/.config/rcommander"
OUT="$TREE/artifacts/update/rcommander-update-stable.json"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

real_pack="${RC_PACK:-}"
if [[ -z "$real_pack" ]]; then
    for candidate in "$ROOT"/target/release/rc-pack "$ROOT"/target/debug/rc-pack; do
        if [[ -x "$candidate" ]]; then
            real_pack="$candidate"
            break
        fi
    done
fi
if [[ -n "$real_pack" ]]; then
    PACK="$real_pack"
else
    PACK="$SCRATCH/rc-pack-stand-in"
    # Records the call; `build` writes its --out, the rest succeeds.
    cat > "$PACK" << 'EOF'
#!/usr/bin/env bash
echo "$*" >> "$RC_PACK_CALLS"
out=""
while [[ $# -gt 0 ]]; do
    [[ "$1" == --out ]] && out="$2"
    shift
done
[[ -z "$out" ]] || echo '{"stand-in":true}' > "$out"
EOF
    chmod +x "$PACK"
fi
export RC_PACK="$PACK" RC_PACK_CALLS="$SCRATCH/calls"

mkdir -p "$TREE/scripts/lib" "$TREE/artifacts/windows" "$KEY_DIR"
cp "$ROOT/scripts/sign-update.sh" "$ROOT/scripts/update-schema-change.sh" "$TREE/scripts/"
cp "$ROOT/scripts/lib/workspace-version.sh" "$ROOT/scripts/lib/lock.sh" "$TREE/scripts/lib/"
printf '%s\n' '[workspace]' '' '[workspace.package]' 'version = "1.2.3"' > "$TREE/Cargo.toml"
printf '%s\n' '# Release notes' '' '## 1.2.3' '' '- Updates install themselves.' > "$TREE/RELEASE-NOTES.md"
git -C "$TREE" init -q
git -C "$TREE" add -A
git -C "$TREE" -c user.name=test -c user.email=test@example.invalid commit -qm tree

package() {
    python3 - "$TREE/artifacts/windows/rcommander-1.2.3-windows-x64.zip" "$1" "$2" << 'EOF'
import sys, zipfile
path, version, profile = sys.argv[1:4]
folder = "rcommander-1.2.3-windows-x64"
with zipfile.ZipFile(path, "w") as archive:
    archive.writestr(f"{folder}/rcommander.exe", "program")
    archive.writestr(
        f"{folder}/VERSION.txt",
        f"rCommander {version}\ncommit abc\nbuilt now\ntarget x86_64-pc-windows-msvc, profile {profile}\n",
    )
EOF
}

sequence_of() {
    python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["payload"]["sequence"])' "$1"
}

no_output() {
    expect_true "$1 writes no file" '[[ ! -e "$TREE/artifacts/update" ]]'
}

package 1.2.3 release
run_status bash "$TREE/scripts/sign-update.sh" stable
expect_status "a missing key stops the run" 1
expect_output "a missing key names where it is looked for" "rcommander-update.key"
no_output "a missing key"

if [[ -n "$real_pack" ]]; then
    "$PACK" keygen --role update --output "$KEY_DIR" > /dev/null
else
    echo "stand-in key" > "$KEY_DIR/rcommander-update.key"
fi

package 1.2.3 release-test
run_status bash "$TREE/scripts/sign-update.sh" stable
expect_status "a package of another profile stops the run" 1
no_output "another profile"

package 1.2.2 release
run_status bash "$TREE/scripts/sign-update.sh" stable
expect_status "a package of another version stops the run" 1
no_output "another version"

package 1.2.3 release
run_status bash "$TREE/scripts/sign-update.sh" beta
expect_status "a plain version on the beta channel stops the run" 1
no_output "the wrong channel"

run_status bash "$TREE/scripts/sign-update.sh" stable
expect_status "a good package is signed" 0
expect_true "the manifest is written" '[[ -s "$OUT" ]]'

if [[ -n "$real_pack" ]]; then
    run_status "$PACK" update manifest verify "$OUT" --public-key "$KEY_DIR/rcommander-update.pub"
    expect_status "the round trip verifies under the key" 0
    before="$(sequence_of "$OUT")"
    sleep 1
    run_status bash "$TREE/scripts/sign-update.sh" stable --bump
    expect_status "--bump signs the manifest again" 0
    after="$(sequence_of "$OUT")"
    expect_true "--bump raises the sequence" '[[ "$after" -gt "$before" ]]'
else
    echo "note: no rc-pack built; the round trip is not run (build it: cargo build -p rc-update --bin rc-pack)"
    expect_true "rc-pack is asked to build" 'grep -q "update manifest build --channel stable --version 1.2.3" "$RC_PACK_CALLS"'
    expect_true "the stand-in was asked to verify" 'grep -q "update manifest verify" "$RC_PACK_CALLS"'
    expect_true "no key path but --key reaches rc-pack" '! grep -q "RCOMMANDER" "$RC_PACK_CALLS"'
fi

finish_tests "sign-update"
