#!/usr/bin/env bash
#
# Builds and signs the update manifest of the packaged release for one channel (RC-100-04):
# takes the archives from artifacts/windows/ (and artifacts/linux/, when there), insists on
# `profile release` and the workspace version in the zip's VERSION.txt, reads the notes from the
# version's section of RELEASE-NOTES.md, works out `schema_change` against the previous release
# (scripts/update-schema-change.sh), signs with rc-pack, verifies the round trip and writes
# artifacts/update/rcommander-update-<channel>.json. Uploading is the owner's step.
#
# Usage:
#   scripts/sign-update.sh <stable|beta>            # build and sign from the packages
#   scripts/sign-update.sh <stable|beta> --bump     # sign the existing manifest again with a new
#                                                   # sequence and expiry (a long stable release)
#
# The key is found by the script itself, never given on a command line: RCOMMANDER_UPDATE_KEY
# (a path), else ~/.config/rcommander/rcommander-update.key. Without it the run stops before it
# touches a file. Its public half beside it (`.pub`, as `rc-pack keygen` writes it) is what the
# round trip verifies against when this build's root is still the placeholder.
#
# Environment:
#   RCOMMANDER_UPDATE_KEY  the private key file
#   RC_PACK                an rc-pack binary to use; without it rc-pack is built (release, under
#                          the build lock)
#
# Adapted from rDownloader's scripts/sign-tools-manifest.sh at commit 26c8592a6.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -z "${RC_PACK:-}" ]]; then
    # shellcheck source=lib/lock.sh
    source "$ROOT/scripts/lib/lock.sh"
    rc_take_lock "$@"
fi
cd "$ROOT"
# shellcheck source=lib/workspace-version.sh
source "$ROOT/scripts/lib/workspace-version.sh"

usage() {
    echo "usage: scripts/sign-update.sh <stable|beta> [--bump]" >&2
    exit 2
}

[[ $# -ge 1 && $# -le 2 ]] || usage
CHANNEL="$1"
BUMP=0
case "$CHANNEL" in stable | beta) ;; *) usage ;; esac
if [[ $# -eq 2 ]]; then
    [[ "$2" == --bump ]] || usage
    BUMP=1
fi

KEY="${RCOMMANDER_UPDATE_KEY:-$HOME/.config/rcommander/rcommander-update.key}"
if [[ ! -f "$KEY" ]]; then
    echo "!! no update signing key: set RCOMMANDER_UPDATE_KEY or put it at" >&2
    echo "   ~/.config/rcommander/rcommander-update.key (rc-pack keygen --role update);" >&2
    echo "   nothing was written" >&2
    exit 1
fi
PUBLIC="${KEY%.key}.pub"
OUT_DIR="$ROOT/artifacts/update"
OUT="$OUT_DIR/rcommander-update-$CHANNEL.json"
VERSION="$(rc_workspace_version < Cargo.toml)"
[[ -n "$VERSION" ]] || {
    echo "!! no workspace version in Cargo.toml" >&2
    exit 1
}

pack() {
    if [[ -n "${RC_PACK:-}" ]]; then
        "$RC_PACK" "$@"
    else
        cargo build --release --locked -p rc-update --bin rc-pack -q
        "$ROOT/target/release/rc-pack" "$@"
    fi
}

verify_args=()
[[ -f "$PUBLIC" ]] && verify_args=(--public-key "$PUBLIC")

if [[ "$BUMP" -eq 1 ]]; then
    [[ -f "$OUT" ]] || {
        echo "!! $OUT does not exist; sign it first without --bump" >&2
        exit 1
    }
    pack update manifest bump "$OUT" --key "$KEY"
    pack update manifest verify "$OUT" "${verify_args[@]}"
    exit 0
fi

# The version's beta-ness decides the channel; a mismatch stops here.
if [[ "$VERSION" == *-* && "$CHANNEL" != beta ]] || [[ "$VERSION" != *-* && "$CHANNEL" != stable ]]; then
    echo "!! $VERSION does not belong on the $CHANNEL channel; nothing was written" >&2
    exit 1
fi

ZIP="$ROOT/artifacts/windows/rcommander-$VERSION-windows-x64.zip"
[[ -f "$ZIP" ]] || {
    echo "!! $ZIP is missing (scripts/package-windows.sh); nothing was written" >&2
    exit 1
}
# The package's VERSION.txt names the version and the release profile.
version_file="$(python3 -c '
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    names = [name for name in archive.namelist() if name.endswith("/VERSION.txt")]
    print(archive.read(names[0]).decode() if names else "")
' "$ZIP")"
if ! grep -qx "rCommander $VERSION" <<< "$version_file"; then
    echo "!! the zip's VERSION.txt is not rCommander $VERSION; nothing was written" >&2
    exit 1
fi
if ! grep -q "profile release$" <<< "$version_file"; then
    echo "!! the zip is no release build (VERSION.txt: profile is not release); nothing was written" >&2
    exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ln -s "$ZIP" "$STAGE/$(basename "$ZIP")"
for tarball in "$ROOT"/artifacts/linux/rcommander-"$VERSION"-linux-*.tar.gz; do
    [[ -f "$tarball" ]] && ln -s "$tarball" "$STAGE/$(basename "$tarball")"
done

# Signed before the tag exists, the release is HEAD on the channel of its version.
release="v$VERSION"
git -C "$ROOT" rev-parse --verify --quiet "refs/tags/$release" > /dev/null || release=HEAD
schema_change="$(RC_SCHEMA_VERSION="v$VERSION" "$ROOT/scripts/update-schema-change.sh" "$release" 2> /dev/null || echo true)"
if [[ "$schema_change" != true && "$schema_change" != false ]]; then
    schema_change=true
fi

mkdir -p "$OUT_DIR"
pack update manifest build \
    --channel "$CHANNEL" \
    --version "$VERSION" \
    --assets "$STAGE" \
    --base-url "https://github.com/degoya/rCommander/releases/download/v$VERSION/" \
    --key "$KEY" \
    --notes "$ROOT/RELEASE-NOTES.md" \
    --schema-change "$schema_change" \
    --out "$OUT"
pack update manifest verify "$OUT" --assets "$STAGE" "${verify_args[@]}"
echo "==> $OUT"
