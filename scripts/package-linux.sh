#!/usr/bin/env bash
#
# Builds the Linux release and packs it:
#   artifacts/linux/rcommander-<version>-linux-<arch>.tar.gz
# x64 natively; arm64 cross-compiled with gcc-aarch64-linux-gnu (the linker is set in
# .cargo/config.toml, the cc crate finds the compiler by the target's prefix), or natively on an
# arm64 machine such as release.yml's runner (RC-100-31), with the system's `cc`.
# with rcommander, README.md, LICENSE, NOTICE (the bundled icons' licence), VERSION.txt and
# plugins/ - the bundled plugins, built, signed and checked by scripts/build-plugins.sh (which
# needs the signing key, RC_PLUGIN_KEY) and installed by rCommander at its first start.
#
# Usage:
#   scripts/package-linux.sh                          # the release profile
#   scripts/package-linux.sh --profile release-test   # a test package, built much faster
#   scripts/package-linux.sh --arch arm64             # for 64-bit ARM (aarch64)
#   scripts/package-linux.sh --plugins <dir>          # the signed packages in <dir> (relative
#                                                     # to the checkout), not built here
#   scripts/package-linux.sh --deb --rpm              # also the deb and the rpm of the tarball
#                                                     # (RC-100-54, scripts/package-deb-rpm.sh):
#                                                     # artifacts/linux/rcommander-<version>-
#                                                     # linux-<arch>.deb and .rpm; needs nfpm
#
# The version is held by scripts/set-version.sh --check first; a package of the same version from
# another commit or a dirty tree stops the run before anything is built (scripts/lib/package.sh;
# RC_REPACK=1 replaces it).
#
# Takes the build lock (scripts/lib/lock.sh) - never wrap it in flock. Parallelism:
# scripts/lib/jobs.sh (RC_JOBS).
set -euo pipefail

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
arch=x64
prebuilt=""
distribution=()
usage="usage: scripts/package-linux.sh [--profile release|release-test] [--arch x64|arm64] [--plugins <dir>] [--deb] [--rpm]"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --profile)
            [[ $# -ge 2 ]] || { echo "$usage" >&2; exit 2; }
            profile="$2"
            shift 2
            ;;
        --arch)
            [[ $# -ge 2 ]] || { echo "$usage" >&2; exit 2; }
            arch="$2"
            shift 2
            ;;
        --plugins)
            [[ $# -ge 2 && -d "$2" ]] || { echo "$usage (an existing directory)" >&2; exit 2; }
            prebuilt="$(cd "$2" && pwd)"
            shift 2
            ;;
        --deb | --rpm)
            distribution+=("$1")
            shift
            ;;
        *) echo "$usage" >&2; exit 2 ;;
    esac
done
case "$profile" in release | release-test) ;; *) echo "!! unknown profile '$profile'" >&2; exit 2 ;; esac
case "$arch" in
    x64) TARGET="x86_64-unknown-linux-gnu" ;;
    arm64)
        TARGET="aarch64-unknown-linux-gnu"
        # On an arm64 machine the target is the host: its own `cc` links, not the cross linker
        # .cargo/config.toml names.
        if [[ "$(uname -m)" == aarch64 || "$(uname -m)" == arm64 ]]; then
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER:-cc}"
        else
            command -v aarch64-linux-gnu-gcc > /dev/null || {
                echo "!! arm64 needs the cross compiler: sudo apt install gcc-aarch64-linux-gnu" >&2
                exit 1
            }
        fi
        ;;
    *) echo "!! unknown arch '$arch'" >&2; exit 2 ;;
esac

version="$(rc_package_version "$ROOT")"
OUT="$ROOT/artifacts/linux"
STAGE="$OUT/rcommander-$version-linux-$arch"
TARBALL="$OUT/rcommander-$version-linux-$arch.tar.gz"
echo "==> packaging rCommander $version for $TARGET (profile: $profile, jobs: $JOBS)"

# The commit and the time, exported so the binary and VERSION.txt name the same ones (build.rs
# takes them verbatim). An existing package of this version from another build stops the run.
commit="$(rc_package_commit)"
rc_package_guard "$TARBALL" "$(basename "$STAGE")/VERSION.txt" "$commit"
RC_BUILD_COMMIT="$commit"
RC_BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
export RC_BUILD_COMMIT RC_BUILD_TIME

plugins=()
plugin_list="$(rc_package_plugins "$ROOT" "$prebuilt")"
while IFS= read -r package; do [[ -z "$package" ]] || plugins+=("$package"); done <<< "$plugin_list"

echo "==> building rcommander"
CARGO_BUILD_JOBS="$JOBS" cargo build --locked --profile "$profile" --target "$TARGET" \
    -j "$JOBS" -p rcommander

binary="${CARGO_TARGET_DIR:-$ROOT/target}/$TARGET/$profile/rcommander"
[[ -f "$binary" ]] || { echo "!! the build left no $binary" >&2; exit 1; }

echo "==> assembling $STAGE"
rm -rf "$STAGE" "$TARBALL"
mkdir -p "$STAGE"
install -m 755 "$binary" "$STAGE/rcommander"
install -m 644 README.md LICENSE NOTICE "$STAGE/"
mkdir -p "$STAGE/plugins"
install -m 644 "${plugins[@]}" "$STAGE/plugins/"
printf '%s\n' "rCommander $version" "commit $RC_BUILD_COMMIT" "built $RC_BUILD_TIME" \
    "target $TARGET, profile $profile" > "$STAGE/VERSION.txt"

echo "==> writing $TARBALL"
tar -C "$OUT" -czf "$TARBALL" "$(basename "$STAGE")"
rm -rf "$STAGE"
if [[ "${#distribution[@]}" -gt 0 ]]; then
    echo "==> the distribution packages of $TARBALL"
    "$ROOT/scripts/package-deb-rpm.sh" "$TARBALL" "$OUT" "${distribution[@]}"
fi
echo "==> done"
ls -la "$TARBALL"
