#!/usr/bin/env bash
#
# Builds the macOS release on a Mac - GitHub's macos-15 runner in release.yml and ci.yml; nothing
# builds it locally (RC-100-44) - and packs it:
#   artifacts/macos/rcommander-<version>-macos-<aarch64|x86_64>.zip
# holding rCommander.app:
#   Contents/Info.plist          bundle id net.rcommander.app, the version from Cargo.toml
#   Contents/MacOS/rcommander    the executable
#   Contents/Resources/          rcommander.icns, README.md, LICENSE, NOTICE, VERSION.txt and
#                                plugins/ - the bundled plugins, which rCommander installs at its
#                                first start (rc-plugin-host looks there inside an app bundle)
# signed ad hoc (`codesign --sign -`), not notarised: Gatekeeper asks once (the wiki's
# installation page). One zip per architecture, no universal binary; x86_64 is cross-compiled on
# the Apple Silicon runner, as rDownloader does.
#
# Usage:
#   scripts/package-macos.sh                           # the release profile, this Mac's arch
#   scripts/package-macos.sh --arch x86_64             # for Intel Macs
#   scripts/package-macos.sh --profile release-test    # a test package, built much faster
#   scripts/package-macos.sh --plugins <dir>           # the signed packages in <dir> (relative
#                                                      # to the checkout), not built here
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

BUNDLE_ID="net.rcommander.app"
# The oldest macOS the binary is built for; Rust's own floor for aarch64-apple-darwin.
MINIMUM_SYSTEM="11.0"

profile=release
arch=""
prebuilt=""
usage="usage: scripts/package-macos.sh [--profile release|release-test] [--arch aarch64|x86_64] [--plugins <dir>]"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --profile | --arch)
            [[ $# -ge 2 ]] || { echo "$usage" >&2; exit 2; }
            if [[ "$1" == --profile ]]; then profile="$2"; else arch="$2"; fi
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
if [[ -z "$arch" ]]; then
    case "$(uname -m)" in arm64 | aarch64) arch=aarch64 ;; *) arch=x86_64 ;; esac
fi
case "$arch" in
    aarch64 | x86_64) TARGET="$arch-apple-darwin" ;;
    *) echo "!! unknown arch '$arch'" >&2; exit 2 ;;
esac
command -v codesign > /dev/null || { echo "!! codesign is missing: this script runs on macOS" >&2; exit 1; }

version="$(rc_package_version "$ROOT")"
# Apple allows at most three integers in both version keys: 1.2.0-beta.1 is 1.2.0 there.
bundle_version="${version%%[-+]*}"
OUT="$ROOT/artifacts/macos"
STAGE="$OUT/rcommander-$version-macos-$arch"
APP="$STAGE/rCommander.app"
ZIP="$OUT/rcommander-$version-macos-$arch.zip"
echo "==> packaging rCommander $version for $TARGET (profile: $profile, jobs: $JOBS)"

# The commit and the time, exported so the binary and VERSION.txt name the same ones (build.rs
# takes them verbatim). An existing package of this version from another build stops the run.
commit="$(rc_package_commit)"
rc_package_guard "$ZIP" "rCommander.app/Contents/Resources/VERSION.txt" "$commit"
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

echo "==> assembling $APP"
rm -rf "$STAGE" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/plugins"
install -m 755 "$binary" "$APP/Contents/MacOS/rcommander"
install -m 644 README.md LICENSE NOTICE "$APP/Contents/Resources/"
if [[ ${#plugins[@]} -gt 0 ]]; then
    install -m 644 "${plugins[@]}" "$APP/Contents/Resources/plugins/"
fi
printf '%s\n' "rCommander $version" "commit $RC_BUILD_COMMIT" "built $RC_BUILD_TIME" \
    "target $TARGET, profile $profile" > "$APP/Contents/Resources/VERSION.txt"
# The brand icon lives beside the crate's resources (RC-100-30 moved it out of docs/, which the
# public export drops); docs/assets/ only in a checkout from before that move.
icon=crates/rcommander/resources/brand/rCommander-icon.png
[[ -f "$icon" ]] || icon=docs/assets/rCommander-icon.png
python3 -I "$ROOT/scripts/lib/icns.py" "$APP/Contents/Resources/rcommander.icns" \
    "$icon" crates/rcommander/resources/rcommander.ico
cat > "$APP/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>rCommander</string>
    <key>CFBundleExecutable</key>
    <string>rcommander</string>
    <key>CFBundleIconFile</key>
    <string>rcommander</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>rCommander</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$bundle_version</string>
    <key>CFBundleVersion</key>
    <string>$bundle_version</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MINIMUM_SYSTEM</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST
if command -v plutil > /dev/null; then
    plutil -lint "$APP/Contents/Info.plist"
fi

echo "==> signing ad hoc"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

echo "==> writing $ZIP"
rc_package_zip "$STAGE" rCommander.app "$ZIP"
rm -rf "$STAGE"
echo "==> done"
ls -la "$ZIP"
