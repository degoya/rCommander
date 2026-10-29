#!/usr/bin/env bash
#
# Builds the Windows release from WSL with cargo-xwin - or natively with cargo on Windows (Git
# Bash, the GitHub runner of release.yml, RC-100-31) - and packs it:
#   artifacts/windows/rcommander-<version>-windows-x64.zip
# with rcommander.exe, README.md, LICENSE, NOTICE (the bundled icons' licence), VERSION.txt and
# plugins/ - the bundled plugins, built, signed and checked by scripts/build-plugins.sh (which
# needs the signing key, RC_PLUGIN_KEY) and installed by rCommander at its first start.
#
# Usage:
#   scripts/package-windows.sh                          # the release profile
#   scripts/package-windows.sh --profile release-test   # a test package, built much faster
#   scripts/package-windows.sh --plugins <dir>          # the signed packages in <dir> (relative
#                                                       # to the checkout), not built here
#
# The version is held by scripts/set-version.sh --check first; a package of the same version from
# another commit or a dirty tree stops the run before anything is built (scripts/lib/package.sh;
# RC_REPACK=1 replaces it).
#
# Takes the build lock (scripts/lib/lock.sh) - never wrap it in flock. Parallelism:
# scripts/lib/jobs.sh (RC_JOBS).
set -euo pipefail

TARGET="x86_64-pc-windows-msvc"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# Sourced before the `cd`, because the lock library resolves this script's own path from $0.
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"
# shellcheck source=lib/package.sh
source "$ROOT/scripts/lib/package.sh"
cd "$ROOT"

profile=release
prebuilt=""
usage="usage: scripts/package-windows.sh [--profile release|release-test] [--plugins <dir>]"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --profile)
            [[ $# -ge 2 ]] || { echo "$usage" >&2; exit 2; }
            profile="$2"
            shift 2
            ;;
        --plugins)
            [[ $# -ge 2 && -d "$2" ]] || { echo "$usage (an existing directory)" >&2; exit 2; }
            prebuilt="$(cd "$2" && pwd)"
            shift 2
            ;;
        *) echo "$usage" >&2; exit 2 ;;
    esac
done
case "$profile" in release | release-test) ;; *) echo "!! unknown profile '$profile'" >&2; exit 2 ;; esac

version="$(rc_package_version "$ROOT")"
OUT="$ROOT/artifacts/windows"
STAGE="$OUT/rcommander-$version-windows-x64"
ZIP="$OUT/rcommander-$version-windows-x64.zip"
echo "==> packaging rCommander $version for $TARGET (profile: $profile, jobs: $JOBS)"

# The commit and the time, exported so the binary and VERSION.txt name the same ones (build.rs
# takes them verbatim). An existing package of this version from another build stops the run.
commit="$(rc_package_commit)"
rc_package_guard "$ZIP" "$(basename "$STAGE")/VERSION.txt" "$commit"
RC_BUILD_COMMIT="$commit"
RC_BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
export RC_BUILD_COMMIT RC_BUILD_TIME

plugins=()
plugin_list="$(rc_package_plugins "$ROOT" "$prebuilt")"
while IFS= read -r package; do [[ -z "$package" ]] || plugins+=("$package"); done <<< "$plugin_list"

# cargo-xwin brings the MSVC libraries to WSL; on Windows itself cargo has them.
build=(xwin build)
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) build=(build) ;; esac
echo "==> building rcommander.exe (cargo ${build[*]})"
CARGO_BUILD_JOBS="$JOBS" cargo "${build[@]}" --locked --profile "$profile" --target "$TARGET" \
    -j "$JOBS" -p rcommander

binary="${CARGO_TARGET_DIR:-$ROOT/target}/$TARGET/$profile/rcommander.exe"
[[ -f "$binary" ]] || { echo "!! the build left no $binary" >&2; exit 1; }

echo "==> assembling $STAGE"
rm -rf "$STAGE" "$ZIP"
mkdir -p "$STAGE"
install -m 755 "$binary" "$STAGE/rcommander.exe"
# A test package's line tables are in the PDB (Cargo.toml, [profile.release-test]); beside the exe
# a panic's backtrace finds it.
if [[ "$profile" == release-test && -f "${binary%.exe}.pdb" ]]; then
    install -m 644 "${binary%.exe}.pdb" "$STAGE/rcommander.pdb"
fi
install -m 644 README.md LICENSE NOTICE "$STAGE/"
mkdir -p "$STAGE/plugins"
install -m 644 "${plugins[@]}" "$STAGE/plugins/"
printf '%s\n' "rCommander $version" "commit $RC_BUILD_COMMIT" "built $RC_BUILD_TIME" \
    "target $TARGET, profile $profile" > "$STAGE/VERSION.txt"

echo "==> writing $ZIP"
rc_package_zip "$OUT" "$(basename "$STAGE")" "$ZIP"
rm -rf "$STAGE"
echo "==> done"
ls -la "$ZIP"
